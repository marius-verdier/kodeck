use std::sync::mpsc::{self, TryRecvError};

use crate::domain::{AnnotationDisposition, AnnotationId, TaskPriority};
use crate::scan::{reconcile_annotations, scan_workspace};
use crate::tui::state::{
    AnnotationReviewState, AnnotationSyncState, AnnotationSyncTrigger, InputMode, InputType,
    StatusMessage, TaskFormState,
};

use super::App;

impl App<'_> {
    pub(super) fn start_annotation_sync(&mut self, trigger: AnnotationSyncTrigger) {
        if self.annotation_sync.is_some() {
            self.status_message = Some(StatusMessage::info("Annotation sync already running"));
            return;
        }
        let root = self.workspace_root.clone();
        let config = self.workspace_config.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let findings = scan_workspace(&root, &config);
            let _ = sender.send(findings);
        });
        self.annotation_sync = Some(AnnotationSyncState { receiver, trigger });
        self.status_message = Some(StatusMessage::info("Syncing annotations…"));
    }

    pub(super) fn poll_annotation_sync(&mut self) {
        let result = match self
            .annotation_sync
            .as_ref()
            .map(|sync| sync.receiver.try_recv())
        {
            Some(Ok(findings)) => Some(Ok(findings)),
            Some(Err(TryRecvError::Disconnected)) => Some(Err(())),
            Some(Err(TryRecvError::Empty)) | None => None,
        };
        let Some(result) = result else {
            return;
        };
        let trigger = self.annotation_sync.take().unwrap().trigger;
        let Ok(findings) = result else {
            self.status_message = Some(StatusMessage::error("Annotation sync failed"));
            return;
        };
        let current = self.cards_file();
        let (next, summary) = reconcile_annotations(&current, findings);
        if let Err(error) = self.private_store.save_cards(&next) {
            self.last_error = Some(error.to_string());
            return;
        }
        self.replace_cards_file(next);
        let should_review = trigger == AnnotationSyncTrigger::Manual || summary.unassigned > 0;
        if should_review {
            self.annotation_review = Some(AnnotationReviewState::default());
        }
        self.status_message = Some(StatusMessage::info(format!(
            "Annotations: {} new, {} updated, {} missing",
            summary.created, summary.updated, summary.disappeared
        )));
    }

    pub(super) fn visible_annotation_ids(&self) -> Vec<crate::domain::AnnotationId> {
        let Some(review) = &self.annotation_review else {
            return Vec::new();
        };
        let mut annotations: Vec<_> = self
            .annotations
            .iter()
            .filter(|annotation| match review.filter {
                crate::tui::state::AnnotationFilter::Unassigned => {
                    annotation.present
                        && matches!(annotation.disposition, AnnotationDisposition::Unassigned)
                }
                crate::tui::state::AnnotationFilter::All => true,
                crate::tui::state::AnnotationFilter::Missing => !annotation.present,
                crate::tui::state::AnnotationFilter::Ignored => {
                    matches!(annotation.disposition, AnnotationDisposition::Ignored)
                }
            })
            .collect();
        annotations.sort_by(|left, right| {
            (
                left.repository_id.as_str(),
                left.path.as_str(),
                left.line,
                left.tag.as_str(),
            )
                .cmp(&(
                    right.repository_id.as_str(),
                    right.path.as_str(),
                    right.line,
                    right.tag.as_str(),
                ))
        });
        annotations
            .into_iter()
            .map(|annotation| annotation.id)
            .collect()
    }

    pub(super) fn move_annotation_focus(&mut self, direction: i8) {
        let count = self.visible_annotation_ids().len();
        let Some(review) = &mut self.annotation_review else {
            return;
        };
        if count == 0 {
            review.focused = 0;
        } else if direction < 0 {
            review.focused = review.focused.saturating_sub(1);
        } else {
            review.focused = review.focused.saturating_add(1).min(count - 1);
        }
    }

    fn targeted_annotation_ids(&self) -> Vec<AnnotationId> {
        let Some(review) = &self.annotation_review else {
            return Vec::new();
        };
        if !review.selected.is_empty() {
            return review.selected.iter().copied().collect();
        }
        self.visible_annotation_ids()
            .get(review.focused)
            .copied()
            .into_iter()
            .collect()
    }

    pub(super) fn toggle_annotation_selection(&mut self) {
        let Some(id) = self
            .annotation_review
            .as_ref()
            .and_then(|review| self.visible_annotation_ids().get(review.focused).copied())
        else {
            return;
        };
        let selectable = self.annotations.iter().any(|annotation| {
            annotation.id == id
                && annotation.present
                && matches!(annotation.disposition, AnnotationDisposition::Unassigned)
        });
        if !selectable {
            self.status_message = Some(StatusMessage::warn(
                "Only present unassigned annotations can be selected",
            ));
            return;
        }
        let review = self.annotation_review.as_mut().unwrap();
        if !review.selected.insert(id) {
            review.selected.remove(&id);
        }
    }

    pub(super) fn cycle_annotation_filter(&mut self) {
        let Some(review) = &mut self.annotation_review else {
            return;
        };
        review.filter = review.filter.next();
        review.focused = 0;
        review.selected.clear();
    }

    pub(super) fn ignore_selected_annotations(&mut self) {
        let ids: std::collections::HashSet<_> =
            self.targeted_annotation_ids().into_iter().collect();
        let mut changed = 0;
        for annotation in &mut self.annotations {
            if ids.contains(&annotation.id)
                && annotation.present
                && matches!(annotation.disposition, AnnotationDisposition::Unassigned)
            {
                annotation.disposition = AnnotationDisposition::Ignored;
                changed += 1;
            }
        }
        let count = self.visible_annotation_ids().len();
        if let Some(review) = &mut self.annotation_review {
            review.selected.clear();
            review.focused = review.focused.min(count.saturating_sub(1));
        }
        if changed == 0 {
            self.status_message = Some(StatusMessage::warn("No unassigned annotation to ignore"));
        } else {
            self.persist_cards();
            self.status_message = Some(StatusMessage::info(format!(
                "Ignored {changed} annotation(s)"
            )));
        }
    }

    pub(super) fn open_annotation_card_form(&mut self) {
        let ids = self.targeted_annotation_ids();
        let mut selected: Vec<_> = self
            .annotations
            .iter()
            .filter(|annotation| {
                ids.contains(&annotation.id)
                    && annotation.present
                    && matches!(annotation.disposition, AnnotationDisposition::Unassigned)
            })
            .cloned()
            .collect();
        selected.sort_by(|left, right| {
            (left.repository_id.as_str(), left.path.as_str(), left.line).cmp(&(
                right.repository_id.as_str(),
                right.path.as_str(),
                right.line,
            ))
        });
        let Some(first) = selected.first() else {
            self.status_message = Some(StatusMessage::warn(
                "Select at least one unassigned annotation",
            ));
            return;
        };
        let mapping = selected
            .iter()
            .filter_map(|annotation| {
                self.workspace_config
                    .annotation_tags
                    .iter()
                    .find(|mapping| mapping.tag.eq_ignore_ascii_case(&annotation.tag))
            })
            .max_by_key(|mapping| priority_rank(mapping.priority));
        let priority = mapping
            .map(|mapping| mapping.priority)
            .unwrap_or(TaskPriority::LOW);
        let target_column = mapping
            .and_then(|mapping| {
                self.columns
                    .iter()
                    .position(|column| column.id == mapping.column_id)
            })
            .unwrap_or(self.focused_column);
        let description = selected
            .iter()
            .map(|annotation| {
                format!(
                    "- [{}] {}:{}:{} — {}",
                    annotation.tag,
                    annotation.repository_id,
                    annotation.path,
                    annotation.line,
                    annotation.message
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        self.task_form = Some(TaskFormState::from_annotations(
            selected.iter().map(|annotation| annotation.id).collect(),
            first.message.clone(),
            description,
            priority,
            target_column,
        ));
        self.active_input = Some(InputType::CreatingTaskTitle);
        self.input_mode = InputMode::Editing;
    }
}

fn priority_rank(priority: TaskPriority) -> u8 {
    match priority {
        TaskPriority::LOW => 0,
        TaskPriority::MEDIUM => 1,
        TaskPriority::HIGH => 2,
    }
}
