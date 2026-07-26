use std::collections::BTreeMap;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;

use crate::domain::{CardId, CardsFile, Column, ColumnConfig, ColumnId, LocalCard, Task};
use crate::tui::state::{
    ArchivedTask, ColumnFormState, Confirmation, ConfirmationState, DetailState, GotoLabelsState,
    GotoTarget, InputMode, InputType, MovePickerState, StatusMessage, TaskFormMode, TaskFormState,
};
use crate::tui::ui::BoardLayout;

use super::App;

impl App<'_> {
    pub(super) fn replace_cards_file(&mut self, cards: CardsFile) {
        let focused_card = self.focused_card_id();
        let cards_by_id: std::collections::HashMap<_, _> =
            cards.cards.iter().map(|card| (card.id, card)).collect();
        let mut placed = std::collections::HashSet::new();
        let mut columns: Vec<_> = self
            .workspace_config
            .columns
            .iter()
            .map(|column| Column::new(column.id.clone(), column.name.clone(), 50))
            .collect();
        for column in &mut columns {
            if let Some(card_ids) = cards.ordering.get(&column.id) {
                for card_id in card_ids {
                    if let Some(card) = cards_by_id.get(card_id)
                        && !card.archived
                    {
                        let order = column.tasks.len();
                        column.tasks.push(Task::from_local_card(card, order));
                        placed.insert(card.id);
                    }
                }
            }
        }
        for card in cards
            .cards
            .iter()
            .filter(|card| !card.archived && !placed.contains(&card.id))
        {
            let target = columns
                .iter()
                .position(|column| column.id == card.column_id)
                .unwrap_or(0);
            let order = columns[target].tasks.len();
            columns[target]
                .tasks
                .push(Task::from_local_card(card, order));
        }
        self.archived_tasks = cards
            .cards
            .iter()
            .filter(|card| card.archived)
            .map(|card| ArchivedTask {
                column_id: card.column_id.clone(),
                task: Task::from_local_card(card, 0),
            })
            .collect();
        self.annotations = cards.annotations;
        self.findings_count = self
            .annotations
            .iter()
            .filter(|annotation| annotation.present)
            .count();
        self.columns = columns;
        self.viewport.ensure_columns(self.columns.len());
        if let Some(card_id) = focused_card.filter(|card_id| self.locate_card(*card_id).is_some()) {
            self.focus_card(card_id);
        } else {
            self.clamp_focus();
        }
    }

    pub(super) fn move_focus_column(&mut self, direction: i8) {
        let target = if direction < 0 {
            self.focused_column.saturating_sub(1)
        } else {
            self.focused_column
                .saturating_add(1)
                .min(self.columns.len().saturating_sub(1))
        };
        self.focus_column(target);
    }

    pub(super) fn move_focused_column(&mut self, direction: i8) {
        let source = self.focused_column;
        let target = if direction < 0 {
            source.checked_sub(1)
        } else {
            source
                .checked_add(1)
                .filter(|target| *target < self.columns.len())
        };
        let Some(target) = target else {
            self.status_message = Some(StatusMessage::info("Column cannot move further"));
            return;
        };

        let mut config = self.workspace_config.clone();
        config.columns.swap(source, target);
        match self.shared_store.save(&config) {
            Ok(()) => {
                self.workspace_config = config;
                self.columns.swap(source, target);
                self.viewport.ensure_columns(self.columns.len());
                self.viewport.card_offsets.swap(source, target);
                self.focused_column = target;
                self.scroll_to_focused_column();
                self.status_message = Some(StatusMessage::info(if direction < 0 {
                    "Column moved left"
                } else {
                    "Column moved right"
                }));
            }
            Err(error) => self.last_error = Some(error.to_string()),
        }
    }

    pub(super) fn focus_column(&mut self, column: usize) {
        self.focused_column = column.min(self.columns.len().saturating_sub(1));
        self.focused_task = self.focused_task.min(
            self.columns[self.focused_column]
                .tasks
                .len()
                .saturating_sub(1),
        );
        self.scroll_to_focused_column();
        self.sync_detail_to_focus();
        self.persist_ui_state();
    }

    pub(super) fn move_focus_task(&mut self, direction: i8) {
        let length = self.columns[self.focused_column].tasks.len();
        if length == 0 {
            return;
        }
        self.focused_task = if direction < 0 {
            self.focused_task.saturating_sub(1)
        } else {
            self.focused_task.saturating_add(1).min(length - 1)
        };
        self.sync_detail_to_focus();
        self.persist_ui_state();
    }

    pub(super) fn focused_card_id(&self) -> Option<CardId> {
        self.columns
            .get(self.focused_column)
            .and_then(|column| column.tasks.get(self.focused_task))
            .map(|task| task.id)
    }

    pub(super) fn target_card_ids(&self) -> Vec<CardId> {
        if self.selected_cards.is_empty() {
            self.focused_card_id().into_iter().collect()
        } else {
            self.columns
                .iter()
                .flat_map(|column| column.tasks.iter())
                .filter(|task| self.selected_cards.contains(&task.id))
                .map(|task| task.id)
                .collect()
        }
    }

    pub(super) fn locate_card(&self, card_id: CardId) -> Option<(usize, usize)> {
        self.columns.iter().enumerate().find_map(|(column, value)| {
            value
                .tasks
                .iter()
                .position(|task| task.id == card_id)
                .map(|task| (column, task))
        })
    }

    pub(super) fn focus_card(&mut self, card_id: CardId) {
        if let Some((column, task)) = self.locate_card(card_id) {
            self.focused_column = column;
            self.focused_task = task;
            self.scroll_to_focused_column();
            self.sync_detail_to_focus();
            self.persist_ui_state();
        }
    }

    pub(super) fn sync_detail_to_focus(&mut self) {
        let card_id = self.focused_card_id();
        if let (Some(detail), Some(card_id)) = (&mut self.detail, card_id) {
            detail.card_id = card_id;
            detail.scroll = 0;
        }
    }

    pub(super) fn toggle_focused_selection(&mut self) {
        let Some(card_id) = self.focused_card_id() else {
            self.status_message = Some(StatusMessage::warn("No card to select"));
            return;
        };
        if !self.selected_cards.insert(card_id) {
            self.selected_cards.remove(&card_id);
        }
    }

    pub(super) fn open_new_card_form(&mut self) {
        self.task_form = Some(TaskFormState::create());
        self.active_input = Some(InputType::CreatingTaskTitle);
        self.input_mode = InputMode::Editing;
    }

    pub(super) fn open_new_column_form(&mut self) {
        self.create_column_popup = Some(ColumnFormState::new());
        self.active_input = Some(InputType::CreatingColumn);
        self.input_mode = InputMode::Editing;
    }

    pub(super) fn open_edit_card_form(&mut self) {
        let Some(task) = self
            .columns
            .get(self.focused_column)
            .and_then(|column| column.tasks.get(self.focused_task))
            .cloned()
        else {
            self.status_message = Some(StatusMessage::warn("No card to edit"));
            return;
        };
        self.detail = None;
        self.task_form = Some(TaskFormState::edit(&task));
        self.active_input = Some(InputType::CreatingTaskTitle);
        self.input_mode = InputMode::Editing;
    }

    pub(super) fn open_card_details(&mut self) {
        let Some(card_id) = self.focused_card_id() else {
            self.status_message = Some(StatusMessage::warn("No card to open"));
            return;
        };
        self.detail = Some(DetailState::new(card_id));
    }

    pub(super) fn move_targets_relative(&mut self, direction: i8) {
        let ids = self.target_card_ids();
        let mut plans = Vec::new();
        for card_id in ids {
            let Some((source, _)) = self.locate_card(card_id) else {
                continue;
            };
            let target = if direction < 0 {
                source.saturating_sub(1)
            } else {
                source
                    .saturating_add(1)
                    .min(self.columns.len().saturating_sub(1))
            };
            if source != target {
                plans.push((card_id, target));
            }
        }
        self.apply_move_plans(plans);
    }

    pub(super) fn move_targets_to_done(&mut self) {
        let Some(done) = self
            .columns
            .iter()
            .position(|column| column.id.as_str() == "done")
        else {
            self.status_message = Some(StatusMessage::warn("The board has no Done column"));
            return;
        };
        let plans = self
            .target_card_ids()
            .into_iter()
            .filter(|card_id| {
                self.locate_card(*card_id)
                    .is_some_and(|(column, _)| column != done)
            })
            .map(|card_id| (card_id, done))
            .collect();
        self.detail = None;
        self.apply_move_plans(plans);
    }

    pub(super) fn apply_move_plans(&mut self, plans: Vec<(CardId, usize)>) {
        if plans.is_empty() {
            self.selected_cards.clear();
            self.status_message = Some(StatusMessage::info("No card can be moved further"));
            return;
        }
        let mut moved = Vec::new();
        for (card_id, target) in plans {
            if let Some((source, task_index)) = self.locate_card(card_id) {
                let task = self.columns[source].tasks.remove(task_index);
                moved.push((task, target));
            }
        }
        let last_id = moved.last().map(|(task, _)| task.id);
        for (task, target) in moved {
            self.columns[target].tasks.push(task);
        }
        self.selected_cards.clear();
        if let Some(card_id) = last_id {
            self.focus_card(card_id);
        }
        self.persist_cards();
    }

    pub(super) fn open_move_picker(&mut self) {
        if self.target_card_ids().is_empty() {
            self.status_message = Some(StatusMessage::warn("No card to move"));
            return;
        }
        self.move_picker = Some(MovePickerState {
            selected_column: self.focused_column,
        });
    }

    pub(super) fn select_picker(&mut self, direction: i8) {
        let Some(picker) = &mut self.move_picker else {
            return;
        };
        picker.selected_column = if direction < 0 {
            picker.selected_column.saturating_sub(1)
        } else {
            picker
                .selected_column
                .saturating_add(1)
                .min(self.columns.len().saturating_sub(1))
        };
    }

    pub(super) fn confirm_active_context(&mut self) {
        if let Some(picker) = self.move_picker.take() {
            let plans = self
                .target_card_ids()
                .into_iter()
                .filter(|card_id| {
                    self.locate_card(*card_id)
                        .is_some_and(|(column, _)| column != picker.selected_column)
                })
                .map(|card_id| (card_id, picker.selected_column))
                .collect();
            self.apply_move_plans(plans);
            return;
        }

        let Some(confirmation) = self.confirmation.take() else {
            return;
        };
        if !confirmation.confirm_selected {
            self.status_message = Some(StatusMessage::info("Action cancelled"));
            return;
        }
        match confirmation.action {
            Confirmation::Archive { card_ids } => self.archive_cards(card_ids),
            Confirmation::DeleteColumn { column_index } => self.delete_column(column_index),
        }
    }

    pub(super) fn request_archive(&mut self) {
        let card_ids = self.target_card_ids();
        if card_ids.is_empty() {
            self.status_message = Some(StatusMessage::warn("No card to archive"));
            return;
        }
        self.confirmation = Some(ConfirmationState {
            action: Confirmation::Archive { card_ids },
            confirm_selected: false,
        });
    }

    pub(super) fn archive_cards(&mut self, card_ids: Vec<CardId>) {
        self.detail = None;
        for card_id in card_ids {
            if let Some((column_index, task_index)) = self.locate_card(card_id) {
                let column_id = self.columns[column_index].id.clone();
                let mut task = self.columns[column_index].tasks.remove(task_index);
                task.archived_by_sync = false;
                self.archived_tasks.push(ArchivedTask { column_id, task });
            }
        }
        self.selected_cards.clear();
        self.clamp_focus();
        self.persist_cards();
        self.persist_ui_state();
    }

    pub(super) fn request_delete_column(&mut self) {
        if self.columns.len() <= 1 {
            self.status_message = Some(StatusMessage::warn(
                "The board must keep at least one column",
            ));
            return;
        }
        if !self.columns[self.focused_column].tasks.is_empty() {
            self.status_message = Some(StatusMessage::warn(
                "Move or archive cards before deleting this column",
            ));
            return;
        }
        self.confirmation = Some(ConfirmationState {
            action: Confirmation::DeleteColumn {
                column_index: self.focused_column,
            },
            confirm_selected: false,
        });
    }

    pub(super) fn clamp_focus(&mut self) {
        self.focused_column = self
            .focused_column
            .min(self.columns.len().saturating_sub(1));
        self.focused_task = self.focused_task.min(
            self.columns[self.focused_column]
                .tasks
                .len()
                .saturating_sub(1),
        );
    }

    pub(super) fn goto_edge_card(&mut self, last: bool) {
        let position = if last {
            self.columns
                .iter()
                .enumerate()
                .rev()
                .find(|(_, column)| !column.tasks.is_empty())
                .map(|(column, value)| (column, value.tasks.len() - 1))
        } else {
            self.columns
                .iter()
                .enumerate()
                .find(|(_, column)| !column.tasks.is_empty())
                .map(|(column, _)| (column, 0))
        };
        let Some((column, task)) = position else {
            self.status_message = Some(StatusMessage::warn("The board has no cards"));
            return;
        };
        self.focused_column = column;
        self.focused_task = task;
        self.scroll_to_focused_column();
        self.persist_ui_state();
    }

    pub(super) fn open_goto_labels(&mut self) {
        const LABEL_KEYS: &[u8] = b"asdfghjklqwertyuiopzxcvbnm";
        let targets: Vec<_> = self
            .visible_cards
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(index, card_id)| {
                let first = *LABEL_KEYS.get(index / LABEL_KEYS.len())?;
                let second = LABEL_KEYS[index % LABEL_KEYS.len()];
                Some(GotoTarget {
                    label: String::from_utf8(vec![first, second]).ok()?,
                    card_id,
                })
            })
            .collect();
        if targets.is_empty() {
            self.status_message = Some(StatusMessage::warn("No visible card to target"));
            return;
        }
        self.goto_labels = Some(GotoLabelsState {
            targets,
            input: String::new(),
        });
    }

    pub(super) fn handle_goto_label_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            self.goto_labels = None;
            return;
        }
        let KeyCode::Char(character) = key.code else {
            return;
        };
        let Some(state) = &mut self.goto_labels else {
            return;
        };
        state.input.push(character.to_ascii_lowercase());
        let matching: Vec<_> = state
            .targets
            .iter()
            .filter(|target| target.label.starts_with(&state.input))
            .cloned()
            .collect();
        match matching.as_slice() {
            [] => {
                self.goto_labels = None;
                self.status_message = Some(StatusMessage::warn("Unknown card label"));
            }
            [target] if state.input.len() == target.label.len() => {
                let card_id = target.card_id;
                self.goto_labels = None;
                self.focus_card(card_id);
            }
            _ => {}
        }
    }

    pub(super) fn save_task_form(&mut self) {
        let Some(form) = &mut self.task_form else {
            return;
        };
        let title = form.title.value().trim().to_owned();
        let description = form.description.lines().join("\n");
        if title.is_empty() {
            form.error = Some("Title is required".to_owned());
            return;
        }
        if description.trim().is_empty() {
            form.error = Some("Description is required".to_owned());
            return;
        }
        let mode = form.mode.clone();
        let priority = form.priority;
        let target_column = form.target_column;

        match mode {
            TaskFormMode::Create => {
                let task = Task::new(
                    title,
                    description,
                    priority,
                    self.columns[self.focused_column].tasks.len(),
                );
                self.columns[self.focused_column].tasks.push(task);
                self.focused_task = self.columns[self.focused_column].tasks.len() - 1;
            }
            TaskFormMode::Edit(card_id) => {
                let Some((column_index, task_index)) = self.locate_card(card_id) else {
                    if let Some(form) = &mut self.task_form {
                        form.error = Some("Card no longer exists".to_owned());
                    }
                    return;
                };
                let task = &mut self.columns[column_index].tasks[task_index];
                task.title = title;
                task.description = description;
                task.priority = priority;
                self.focused_column = column_index;
                self.focused_task = task_index;
            }
            TaskFormMode::CreateFromAnnotations(annotation_ids) => {
                let column_index = target_column
                    .unwrap_or(self.focused_column)
                    .min(self.columns.len().saturating_sub(1));
                let task = Task::new(
                    title,
                    description,
                    priority,
                    self.columns[column_index].tasks.len(),
                );
                let card_id = task.id;
                self.columns[column_index].tasks.push(task);
                self.focused_column = column_index;
                self.focused_task = self.columns[column_index].tasks.len() - 1;
                for annotation in &mut self.annotations {
                    if annotation_ids.contains(&annotation.id) {
                        annotation.disposition =
                            crate::domain::AnnotationDisposition::Linked { card_id };
                    }
                }
                if let Some(review) = &mut self.annotation_review {
                    review.selected.clear();
                }
            }
        }

        self.active_input = None;
        self.input_mode = InputMode::Normal;
        self.task_form = None;
        self.persist_cards();
        self.persist_ui_state();
    }

    pub(super) fn create_column(&mut self, name: String) {
        let name = name.trim();
        if name.is_empty() {
            self.last_error = Some("column name must not be empty".to_owned());
            return;
        }

        let base = slugify(name);
        let mut id = base.clone();
        let mut suffix = 2;
        while self
            .workspace_config
            .columns
            .iter()
            .any(|column| column.id.as_str() == id)
        {
            id = format!("{base}-{suffix}");
            suffix += 1;
        }

        let column = ColumnConfig {
            id: ColumnId::new(id),
            name: name.to_owned(),
        };
        let mut config = self.workspace_config.clone();
        config.columns.push(column.clone());
        match self.shared_store.save(&config) {
            Ok(()) => {
                self.workspace_config = config;
                self.columns.push(Column::new(column.id, column.name, 50));
                self.last_error = None;
            }
            Err(error) => self.last_error = Some(error.to_string()),
        }
    }

    pub(super) fn cards_file(&self) -> CardsFile {
        let mut cards = Vec::new();
        let mut ordering = BTreeMap::new();
        for column in &self.columns {
            let ids: Vec<_> = column.tasks.iter().map(|task| task.id).collect();
            ordering.insert(column.id.clone(), ids);
            cards.extend(column.tasks.iter().map(|task| LocalCard {
                id: task.id,
                title: task.title.clone(),
                description: task.description.clone(),
                priority: task.priority,
                column_id: column.id.clone(),
                archived: false,
                archived_by_sync: task.archived_by_sync,
            }));
        }
        cards.extend(self.archived_tasks.iter().map(|archived| LocalCard {
            id: archived.task.id,
            title: archived.task.title.clone(),
            description: archived.task.description.clone(),
            priority: archived.task.priority,
            column_id: archived.column_id.clone(),
            archived: true,
            archived_by_sync: archived.task.archived_by_sync,
        }));

        CardsFile {
            schema_version: crate::domain::PRIVATE_SCHEMA_VERSION,
            workspace_id: self.workspace_config.id,
            cards,
            ordering,
            annotations: self.annotations.clone(),
        }
    }

    pub(super) fn persist_cards(&mut self) {
        let cards = self.cards_file();
        match self.private_store.save_cards(&cards) {
            Ok(()) => self.last_error = None,
            Err(error) => self.last_error = Some(error.to_string()),
        }
    }

    pub(super) fn persist_ui_state(&mut self) {
        self.ui_state.selected_column_id = self
            .columns
            .get(self.focused_column)
            .map(|column| column.id.clone());
        self.ui_state.selected_card_id = self
            .columns
            .get(self.focused_column)
            .and_then(|column| column.tasks.get(self.focused_task))
            .map(|task| task.id);
        match self.private_store.save_ui_state(&self.ui_state) {
            Ok(()) => self.last_error = None,
            Err(error) => self.last_error = Some(error.to_string()),
        }
    }

    pub(super) fn delete_column(&mut self, column_index: usize) {
        if self.columns.len() <= 1 {
            self.last_error = Some("the board must keep at least one column".to_owned());
            return;
        }
        let Some(column) = self.columns.get(column_index) else {
            return;
        };
        if !column.tasks.is_empty() {
            self.last_error = Some("move or archive cards before deleting this column".to_owned());
            return;
        }

        let column_id = column.id.clone();
        let mut config = self.workspace_config.clone();
        config.columns.retain(|column| column.id != column_id);
        match self.shared_store.save(&config) {
            Ok(()) => {
                self.workspace_config = config;
                self.columns.remove(column_index);
                if self.focused_column >= self.columns.len() {
                    self.focused_column = self.columns.len() - 1;
                }
                self.focused_task = 0;
                self.last_error = None;
                self.persist_cards();
            }
            Err(error) => self.last_error = Some(error.to_string()),
        }
    }

    pub(super) fn scroll_to_focused_column(&mut self) {
        self.viewport.ensure_columns(self.columns.len());
        let visible = BoardLayout::compute(
            Rect::new(0, 0, 80, 24),
            self.columns.len(),
            self.detail.is_some(),
        )
        .visible_columns;
        self.viewport
            .reveal_column(self.focused_column, visible, self.columns.len());
    }
}

fn slugify(value: &str) -> String {
    let mut result = String::new();
    let mut separator = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            result.push(character.to_ascii_lowercase());
            separator = false;
        } else if !separator && !result.is_empty() {
            result.push('-');
            separator = true;
        }
    }
    while result.ends_with('-') {
        result.pop();
    }
    if result.is_empty() {
        "column".to_owned()
    } else {
        result
    }
}
