use crossterm::event::{self, KeyCode, KeyEvent, KeyEventKind};
use ratatui_textarea::Input;
use tui_input::backend::crossterm::EventHandler;

use crate::domain::TaskPriority;

use super::App;
use crate::tui::keymap::{self, Action, KeyContext};
use crate::tui::state::{AnnotationSyncTrigger, InputMode, InputType, StatusMessage};

impl App<'_> {
    pub(super) fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.kind != KeyEventKind::Press {
            return false;
        }

        if self.goto_labels.is_some() {
            self.handle_goto_label_key(key);
            return false;
        }

        let context = self.key_context();
        if context == KeyContext::Form {
            self.handle_form_key(key);
            return false;
        }

        let Some(action) = keymap::resolve(context, key) else {
            if context == KeyContext::Goto {
                self.goto_pending = false;
                self.status_message = Some(StatusMessage::warn("Unknown goto command"));
            }
            return false;
        };
        self.execute_action(action)
    }

    pub(super) fn key_context(&self) -> KeyContext {
        if self.help_open {
            KeyContext::Help
        } else if self.confirmation.is_some() {
            KeyContext::Confirmation
        } else if self.move_picker.is_some() {
            KeyContext::Picker
        } else if self.goto_pending {
            KeyContext::Goto
        } else if self.create_column_popup.is_some() || self.task_form.is_some() {
            KeyContext::Form
        } else if self.annotation_review.is_some() {
            KeyContext::Annotations
        } else if self.detail.is_some() {
            KeyContext::Details
        } else {
            KeyContext::Board
        }
    }

    pub(super) fn execute_action(&mut self, action: Action) -> bool {
        self.status_message = None;
        self.last_error = None;
        match action {
            Action::Quit => return true,
            Action::ShowHelp => {
                self.help_open = true;
                self.help_scroll = 0;
            }
            Action::Close => self.close_active_context(),
            Action::MoveFocusLeft => self.move_focus_column(-1),
            Action::MoveFocusRight => self.move_focus_column(1),
            Action::MoveFocusUp => self.move_focus_task(-1),
            Action::MoveFocusDown => self.move_focus_task(1),
            Action::NewCard => self.open_new_card_form(),
            Action::NewColumn => self.open_new_column_form(),
            Action::EditCard => self.open_edit_card_form(),
            Action::ViewCard => self.open_card_details(),
            Action::MarkDone => self.move_targets_to_done(),
            Action::RequestArchive => self.request_archive(),
            Action::RequestDeleteColumn => self.request_delete_column(),
            Action::MoveCardsLeft => self.move_targets_relative(-1),
            Action::MoveCardsRight => self.move_targets_relative(1),
            Action::MoveColumnLeft => self.move_focused_column(-1),
            Action::MoveColumnRight => self.move_focused_column(1),
            Action::OpenMovePicker => self.open_move_picker(),
            Action::ToggleSelection if self.annotation_review.is_some() => {
                self.toggle_annotation_selection();
            }
            Action::ToggleSelection => self.toggle_focused_selection(),
            Action::SyncAnnotations => {
                self.start_annotation_sync(AnnotationSyncTrigger::Manual);
            }
            Action::AnnotationPrevious => self.move_annotation_focus(-1),
            Action::AnnotationNext => self.move_annotation_focus(1),
            Action::CreateFromAnnotations => self.open_annotation_card_form(),
            Action::IgnoreAnnotations => self.ignore_selected_annotations(),
            Action::CycleAnnotationFilter => self.cycle_annotation_filter(),
            Action::EnterGoto => self.goto_pending = true,
            Action::GotoFirstCard => {
                self.goto_pending = false;
                self.goto_edge_card(false);
            }
            Action::GotoLastCard => {
                self.goto_pending = false;
                self.goto_edge_card(true);
            }
            Action::GotoFirstColumn => {
                self.goto_pending = false;
                self.focus_column(0);
            }
            Action::GotoLastColumn => {
                self.goto_pending = false;
                self.focus_column(self.columns.len().saturating_sub(1));
            }
            Action::GotoVisibleCard => {
                self.goto_pending = false;
                self.open_goto_labels();
            }
            Action::SelectPrevious => self.select_picker(-1),
            Action::SelectNext => self.select_picker(1),
            Action::Accept => {
                if let Some(confirmation) = &mut self.confirmation {
                    confirmation.confirm_selected = true;
                }
                self.confirm_active_context();
            }
            Action::Confirm => self.confirm_active_context(),
            Action::Reject => self.confirmation = None,
            Action::ToggleChoice => {
                if let Some(confirmation) = &mut self.confirmation {
                    confirmation.confirm_selected = !confirmation.confirm_selected;
                }
            }
            Action::ScrollUp if self.detail.is_some() && !self.help_open => {
                if let Some(detail) = &mut self.detail {
                    detail.scroll = detail.scroll.saturating_sub(4);
                }
            }
            Action::ScrollDown if self.detail.is_some() && !self.help_open => {
                if let Some(detail) = &mut self.detail {
                    detail.scroll = detail.scroll.saturating_add(4);
                }
            }
            Action::ScrollUp => self.help_scroll = self.help_scroll.saturating_sub(1),
            Action::ScrollDown => {
                self.help_scroll = self.help_scroll.saturating_add(1);
            }
            Action::SaveForm | Action::NextField | Action::PreviousField | Action::FormEnter => {}
        }
        false
    }

    pub(super) fn handle_form_key(&mut self, key: KeyEvent) {
        self.status_message = None;
        self.last_error = None;
        if let Some(action) = keymap::resolve(KeyContext::Form, key) {
            match action {
                Action::SaveForm => self.save_active_form(),
                Action::NextField => self.cycle_form_field(1),
                Action::PreviousField => self.cycle_form_field(-1),
                Action::FormEnter => self.handle_form_enter(),
                Action::Close => self.close_form(),
                _ => {}
            }
            return;
        }

        if matches!(
            self.active_input,
            Some(InputType::CreatingTaskPriority | InputType::CreatingTaskColumn)
        ) {
            match key.code {
                KeyCode::Left
                    if matches!(self.active_input, Some(InputType::CreatingTaskPriority)) =>
                {
                    self.rotate_priority(-1);
                }
                KeyCode::Right
                    if matches!(self.active_input, Some(InputType::CreatingTaskPriority)) =>
                {
                    self.rotate_priority(1);
                }
                KeyCode::Left => self.rotate_target_column(-1),
                KeyCode::Right => self.rotate_target_column(1),
                _ => {}
            }
            return;
        }

        let event = event::Event::Key(key);
        match self.active_input {
            Some(InputType::CreatingColumn) => {
                if let Some(popup) = &mut self.create_column_popup {
                    popup.input.handle_event(&event);
                    popup.error = None;
                }
            }
            Some(InputType::CreatingTaskTitle) => {
                if let Some(popup) = &mut self.task_form {
                    popup.title.handle_event(&event);
                    popup.error = None;
                }
            }
            Some(InputType::CreatingTaskDescription) => {
                let input: Input = event.into();
                if let Some(popup) = &mut self.task_form {
                    popup.description.input(input);
                    popup.error = None;
                }
            }
            Some(InputType::CreatingTaskPriority | InputType::CreatingTaskColumn) | None => {}
        }
    }

    pub(super) fn handle_form_enter(&mut self) {
        match self.active_input {
            Some(InputType::CreatingTaskTitle) => {
                self.active_input = Some(InputType::CreatingTaskDescription);
            }
            Some(InputType::CreatingTaskDescription) => {
                let input: Input = event::Event::Key(KeyEvent::new(
                    KeyCode::Enter,
                    crossterm::event::KeyModifiers::NONE,
                ))
                .into();
                if let Some(popup) = &mut self.task_form {
                    popup.description.input(input);
                }
            }
            Some(
                InputType::CreatingColumn
                | InputType::CreatingTaskPriority
                | InputType::CreatingTaskColumn,
            )
            | None => {}
        }
    }

    pub(super) fn cycle_form_field(&mut self, direction: i8) {
        let has_column = self
            .task_form
            .as_ref()
            .is_some_and(|form| form.target_column.is_some());
        self.active_input = match (self.active_input, direction, has_column) {
            (Some(InputType::CreatingTaskTitle), 1, _) => Some(InputType::CreatingTaskDescription),
            (Some(InputType::CreatingTaskDescription), 1, _) => {
                Some(InputType::CreatingTaskPriority)
            }
            (Some(InputType::CreatingTaskPriority), 1, true) => Some(InputType::CreatingTaskColumn),
            (Some(InputType::CreatingTaskPriority), 1, false)
            | (Some(InputType::CreatingTaskColumn), 1, _) => Some(InputType::CreatingTaskTitle),
            (Some(InputType::CreatingTaskTitle), -1, true) => Some(InputType::CreatingTaskColumn),
            (Some(InputType::CreatingTaskTitle), -1, false) => {
                Some(InputType::CreatingTaskPriority)
            }
            (Some(InputType::CreatingTaskDescription), -1, _) => Some(InputType::CreatingTaskTitle),
            (Some(InputType::CreatingTaskPriority), -1, _) => {
                Some(InputType::CreatingTaskDescription)
            }
            (Some(InputType::CreatingTaskColumn), -1, _) => Some(InputType::CreatingTaskPriority),
            (current, _, _) => current,
        };
    }

    pub(super) fn rotate_priority(&mut self, direction: i8) {
        let rotate = |priority| match (priority, direction) {
            (TaskPriority::LOW, 1) | (TaskPriority::HIGH, -1) => TaskPriority::MEDIUM,
            (TaskPriority::MEDIUM, 1) | (TaskPriority::LOW, -1) => TaskPriority::HIGH,
            (TaskPriority::HIGH, 1) | (TaskPriority::MEDIUM, -1) => TaskPriority::LOW,
            (priority, _) => priority,
        };
        if let Some(popup) = &mut self.task_form {
            popup.priority = rotate(popup.priority);
            popup.error = None;
        }
    }

    pub(super) fn rotate_target_column(&mut self, direction: i8) {
        let Some(form) = &mut self.task_form else {
            return;
        };
        let Some(column) = &mut form.target_column else {
            return;
        };
        *column = if direction < 0 {
            column.saturating_sub(1)
        } else {
            column
                .saturating_add(1)
                .min(self.columns.len().saturating_sub(1))
        };
        form.error = None;
    }

    pub(super) fn save_active_form(&mut self) {
        if let Some(popup) = &self.create_column_popup {
            let name = popup.input.value().to_owned();
            if name.trim().is_empty() {
                if let Some(popup) = &mut self.create_column_popup {
                    popup.error = Some("Column name is required".to_owned());
                }
                return;
            }
            let before = self.columns.len();
            self.create_column(name);
            if self.columns.len() > before {
                self.create_column_popup = None;
                self.active_input = None;
                self.input_mode = InputMode::Normal;
            }
        } else if self.task_form.is_some() {
            self.save_task_form();
        }
    }

    pub(super) fn close_form(&mut self) {
        self.create_column_popup = None;
        self.task_form = None;
        self.active_input = None;
        self.input_mode = InputMode::Normal;
        self.status_message = Some(StatusMessage::info("Changes discarded"));
    }

    pub(super) fn close_active_context(&mut self) {
        if self.help_open {
            self.help_open = false;
        } else if self.goto_pending {
            self.goto_pending = false;
        } else if self.move_picker.is_some() {
            self.move_picker = None;
        } else if self.annotation_review.is_some() {
            self.annotation_review = None;
        } else if self.detail.is_some() {
            self.detail = None;
        } else if !self.selected_cards.is_empty() {
            self.selected_cards.clear();
        }
    }
}
