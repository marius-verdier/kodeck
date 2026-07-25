use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use color_eyre::eyre::Result;
use crossterm::event;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap,
};
use ratatui::{DefaultTerminal, Frame};
use ratatui_textarea::Input;
use tui_input::backend::crossterm::EventHandler;

use crate::domain::{
    CardId, CardsFile, Column, ColumnConfig, ColumnId, LocalCard, Task, TaskPriority, UiStateFile,
    WorkspaceConfig,
};
use crate::workspace::{PrivateWorkspaceStore, SharedWorkspaceStore, WorkspaceContext};

use super::keymap::{self, Action, KeyContext};
use super::popups::CreateColumnPopup;
use super::state::{
    DetailState, InputMode, InputType, StatusLevel, StatusMessage, TaskFormMode, TaskFormState,
};
use super::ui::{
    BoardLayout, DetailPlacement, UiTheme, ViewportState, abbreviated_path, modal_area, truncate,
};

struct ArchivedTask {
    column_id: ColumnId,
    task: Task,
}

#[derive(Debug, Clone)]
enum Confirmation {
    Archive { card_ids: Vec<CardId> },
    DeleteColumn { column_index: usize },
}

#[derive(Debug, Clone)]
struct ConfirmationState {
    action: Confirmation,
    confirm_selected: bool,
}

#[derive(Debug, Clone)]
struct MovePickerState {
    selected_column: usize,
}

#[derive(Debug, Clone)]
struct GotoTarget {
    label: String,
    card_id: CardId,
}

#[derive(Debug, Clone)]
struct GotoLabelsState {
    targets: Vec<GotoTarget>,
    input: String,
}

pub struct App<'a> {
    workspace_root: PathBuf,
    workspace_config: WorkspaceConfig,
    shared_store: SharedWorkspaceStore,
    private_store: PrivateWorkspaceStore,
    ui_state: UiStateFile,
    findings_count: usize,
    last_error: Option<String>,
    input_mode: InputMode,
    active_input: Option<InputType>,
    viewport: ViewportState,
    theme: UiTheme,
    selected_cards: HashSet<CardId>,
    confirmation: Option<ConfirmationState>,
    move_picker: Option<MovePickerState>,
    goto_pending: bool,
    goto_labels: Option<GotoLabelsState>,
    help_open: bool,
    help_scroll: usize,
    visible_cards: Vec<CardId>,
    status_message: Option<StatusMessage>,
    // COLUMNS MANAGEMENT
    columns: Vec<Column>,
    focused_column: usize,
    create_column_popup: Option<CreateColumnPopup>,
    // TASKS MANAGEMENT
    focused_task: usize,
    task_form: Option<TaskFormState<'a>>,
    detail: Option<DetailState>,
    archived_tasks: Vec<ArchivedTask>,
}

impl App<'_> {
    pub fn from_workspace(context: WorkspaceContext, findings_count: usize) -> Self {
        let mut warnings = context.warnings.clone();
        let mut columns: Vec<_> = context
            .config
            .columns
            .iter()
            .map(|column| Column::new(column.id.clone(), column.name.clone(), 50))
            .collect();
        let cards = &context.private_state.cards;
        let cards_by_id: HashMap<_, _> = cards.cards.iter().map(|card| (card.id, card)).collect();
        let mut placed = HashSet::new();

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
            if !columns.iter().any(|column| column.id == card.column_id) {
                warnings.push(format!(
                    "card '{}' references missing column '{}'; displayed in '{}'",
                    card.title, card.column_id, columns[target].name
                ));
            }
            let order = columns[target].tasks.len();
            columns[target]
                .tasks
                .push(Task::from_local_card(card, order));
        }

        let archived_tasks: Vec<ArchivedTask> = cards
            .cards
            .iter()
            .filter(|card| card.archived)
            .map(|card| ArchivedTask {
                column_id: card.column_id.clone(),
                task: Task::from_local_card(card, 0),
            })
            .collect();

        let focused_column = context
            .private_state
            .state
            .selected_column_id
            .as_ref()
            .and_then(|id| columns.iter().position(|column| &column.id == id))
            .unwrap_or(0);
        let focused_task = context
            .private_state
            .state
            .selected_card_id
            .and_then(|id| {
                columns[focused_column]
                    .tasks
                    .iter()
                    .position(|task| task.id == id)
            })
            .unwrap_or(0);
        let shared_store = context.shared_store().clone();
        let private_store = context.private_store().clone();
        let ui_state = context.private_state.state.clone();
        let column_count = columns.len();
        let initial_status = warnings.first().cloned().map(StatusMessage::warn);

        Self {
            workspace_root: context.root,
            workspace_config: context.config,
            shared_store,
            private_store,
            ui_state,
            findings_count,
            last_error: None,
            input_mode: InputMode::Normal,
            active_input: None,
            columns,
            viewport: ViewportState {
                first_column: focused_column,
                card_offsets: vec![0; column_count],
            },
            theme: UiTheme::default(),
            selected_cards: HashSet::new(),
            confirmation: None,
            move_picker: None,
            goto_pending: false,
            goto_labels: None,
            help_open: false,
            help_scroll: 0,
            visible_cards: Vec::new(),
            status_message: initial_status,
            focused_column,
            focused_task,
            create_column_popup: None,
            task_form: None,
            detail: None,
            archived_tasks,
        }
    }

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        loop {
            terminal.draw(|frame| self.render(frame))?;
            if let Some(key) = event::read()?.as_key_press_event()
                && self.handle_key(key)
            {
                self.persist_ui_state();
                return Ok(());
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
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

    fn key_context(&self) -> KeyContext {
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
        } else if self.detail.is_some() {
            KeyContext::Details
        } else {
            KeyContext::Board
        }
    }

    fn execute_action(&mut self, action: Action) -> bool {
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
            Action::ToggleSelection => self.toggle_focused_selection(),
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

    fn handle_form_key(&mut self, key: KeyEvent) {
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

        if matches!(self.active_input, Some(InputType::CreatingTaskPriority)) {
            match key.code {
                KeyCode::Left => self.rotate_priority(-1),
                KeyCode::Right => self.rotate_priority(1),
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
            Some(InputType::CreatingTaskPriority) | None => {}
        }
    }

    fn handle_form_enter(&mut self) {
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
            Some(InputType::CreatingColumn) | Some(InputType::CreatingTaskPriority) | None => {}
        }
    }

    fn cycle_form_field(&mut self, direction: i8) {
        self.active_input = match (self.active_input, direction) {
            (Some(InputType::CreatingTaskTitle), 1) => Some(InputType::CreatingTaskDescription),
            (Some(InputType::CreatingTaskDescription), 1) => Some(InputType::CreatingTaskPriority),
            (Some(InputType::CreatingTaskPriority), 1) => Some(InputType::CreatingTaskTitle),
            (Some(InputType::CreatingTaskTitle), -1) => Some(InputType::CreatingTaskPriority),
            (Some(InputType::CreatingTaskDescription), -1) => Some(InputType::CreatingTaskTitle),
            (Some(InputType::CreatingTaskPriority), -1) => Some(InputType::CreatingTaskDescription),
            (current, _) => current,
        };
    }

    fn rotate_priority(&mut self, direction: i8) {
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

    fn save_active_form(&mut self) {
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

    fn close_form(&mut self) {
        self.create_column_popup = None;
        self.task_form = None;
        self.active_input = None;
        self.input_mode = InputMode::Normal;
        self.status_message = Some(StatusMessage::info("Changes discarded"));
    }

    fn close_active_context(&mut self) {
        if self.help_open {
            self.help_open = false;
        } else if self.goto_pending {
            self.goto_pending = false;
        } else if self.move_picker.is_some() {
            self.move_picker = None;
        } else if self.detail.is_some() {
            self.detail = None;
        } else if !self.selected_cards.is_empty() {
            self.selected_cards.clear();
        }
    }

    fn move_focus_column(&mut self, direction: i8) {
        let target = if direction < 0 {
            self.focused_column.saturating_sub(1)
        } else {
            self.focused_column
                .saturating_add(1)
                .min(self.columns.len().saturating_sub(1))
        };
        self.focus_column(target);
    }

    fn move_focused_column(&mut self, direction: i8) {
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

    fn focus_column(&mut self, column: usize) {
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

    fn move_focus_task(&mut self, direction: i8) {
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

    fn focused_card_id(&self) -> Option<CardId> {
        self.columns
            .get(self.focused_column)
            .and_then(|column| column.tasks.get(self.focused_task))
            .map(|task| task.id)
    }

    fn target_card_ids(&self) -> Vec<CardId> {
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

    fn locate_card(&self, card_id: CardId) -> Option<(usize, usize)> {
        self.columns.iter().enumerate().find_map(|(column, value)| {
            value
                .tasks
                .iter()
                .position(|task| task.id == card_id)
                .map(|task| (column, task))
        })
    }

    fn focus_card(&mut self, card_id: CardId) {
        if let Some((column, task)) = self.locate_card(card_id) {
            self.focused_column = column;
            self.focused_task = task;
            self.scroll_to_focused_column();
            self.sync_detail_to_focus();
            self.persist_ui_state();
        }
    }

    fn sync_detail_to_focus(&mut self) {
        let card_id = self.focused_card_id();
        if let (Some(detail), Some(card_id)) = (&mut self.detail, card_id) {
            detail.card_id = card_id;
            detail.scroll = 0;
        }
    }

    fn toggle_focused_selection(&mut self) {
        let Some(card_id) = self.focused_card_id() else {
            self.status_message = Some(StatusMessage::warn("No card to select"));
            return;
        };
        if !self.selected_cards.insert(card_id) {
            self.selected_cards.remove(&card_id);
        }
    }

    fn open_new_card_form(&mut self) {
        self.task_form = Some(TaskFormState::create());
        self.active_input = Some(InputType::CreatingTaskTitle);
        self.input_mode = InputMode::Editing;
    }

    fn open_new_column_form(&mut self) {
        self.create_column_popup = Some(CreateColumnPopup::new());
        self.active_input = Some(InputType::CreatingColumn);
        self.input_mode = InputMode::Editing;
    }

    fn open_edit_card_form(&mut self) {
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

    fn open_card_details(&mut self) {
        let Some(card_id) = self.focused_card_id() else {
            self.status_message = Some(StatusMessage::warn("No card to open"));
            return;
        };
        self.detail = Some(DetailState::new(card_id));
    }

    fn move_targets_relative(&mut self, direction: i8) {
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

    fn move_targets_to_done(&mut self) {
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

    fn apply_move_plans(&mut self, plans: Vec<(CardId, usize)>) {
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

    fn open_move_picker(&mut self) {
        if self.target_card_ids().is_empty() {
            self.status_message = Some(StatusMessage::warn("No card to move"));
            return;
        }
        self.move_picker = Some(MovePickerState {
            selected_column: self.focused_column,
        });
    }

    fn select_picker(&mut self, direction: i8) {
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

    fn confirm_active_context(&mut self) {
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

    fn request_archive(&mut self) {
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

    fn archive_cards(&mut self, card_ids: Vec<CardId>) {
        self.detail = None;
        for card_id in card_ids {
            if let Some((column_index, task_index)) = self.locate_card(card_id) {
                let column_id = self.columns[column_index].id.clone();
                let task = self.columns[column_index].tasks.remove(task_index);
                self.archived_tasks.push(ArchivedTask { column_id, task });
            }
        }
        self.selected_cards.clear();
        self.clamp_focus();
        self.persist_cards();
        self.persist_ui_state();
    }

    fn request_delete_column(&mut self) {
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

    fn clamp_focus(&mut self) {
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

    fn goto_edge_card(&mut self, last: bool) {
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

    fn open_goto_labels(&mut self) {
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

    fn handle_goto_label_key(&mut self, key: KeyEvent) {
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

    fn save_task_form(&mut self) {
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
        let mode = form.mode;
        let priority = form.priority;

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
        }

        self.active_input = None;
        self.input_mode = InputMode::Normal;
        self.task_form = None;
        self.persist_cards();
        self.persist_ui_state();
    }

    fn create_column(&mut self, name: String) {
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

    fn cards_file(&self) -> CardsFile {
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
            }));
        }
        cards.extend(self.archived_tasks.iter().map(|archived| LocalCard {
            id: archived.task.id,
            title: archived.task.title.clone(),
            description: archived.task.description.clone(),
            priority: archived.task.priority,
            column_id: archived.column_id.clone(),
            archived: true,
        }));

        CardsFile {
            schema_version: crate::domain::PRIVATE_SCHEMA_VERSION,
            workspace_id: self.workspace_config.id,
            cards,
            ordering,
        }
    }

    fn persist_cards(&mut self) {
        let cards = self.cards_file();
        match self.private_store.save_cards(&cards) {
            Ok(()) => self.last_error = None,
            Err(error) => self.last_error = Some(error.to_string()),
        }
    }

    fn persist_ui_state(&mut self) {
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

    fn render(&mut self, frame: &mut Frame) {
        let layout = BoardLayout::compute(frame.area(), self.columns.len(), self.detail.is_some());
        if layout.too_small {
            frame.render_widget(
                Paragraph::new("Terminal too small")
                    .alignment(Alignment::Center)
                    .style(self.theme.focus()),
                frame.area(),
            );
            return;
        }

        self.viewport.ensure_columns(self.columns.len());
        self.viewport.reveal_column(
            self.focused_column,
            layout.visible_columns,
            self.columns.len(),
        );
        let card_capacity = usize::from(layout.board.height.saturating_sub(2) / 2).max(1);
        self.viewport
            .reveal_card(self.focused_column, self.focused_task, card_capacity);

        self.render_header(frame, &layout);
        self.render_board(frame, &layout);
        if let Some(detail_area) = layout.detail {
            self.render_detail(frame, detail_area, false);
        }
        self.render_status(frame, &layout);
        self.render_footer(frame, &layout);

        if layout.detail_placement == DetailPlacement::Overlay && self.detail.is_some() {
            let area = modal_area(
                layout.board,
                layout.board.width.saturating_sub(4),
                layout.board.height,
            );
            self.render_detail(frame, area, true);
        }
        self.render_confirmation(frame, layout.board);
        self.render_move_picker(frame, layout.board);
        self.render_goto_menu(frame, layout.board);
        self.render_help(frame, layout.board);
        self.render_column_form(frame, layout.board);
        self.render_task_form(frame, layout.board);
    }

    fn render_header(&self, frame: &mut Frame, layout: &BoardLayout) {
        let card_count: usize = self.columns.iter().map(|column| column.tasks.len()).sum();
        let title = Line::from(vec![
            Span::styled("KODECK", self.theme.focus()),
            Span::raw("  "),
            Span::styled(
                self.workspace_config.name.as_str(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]);
        let summary = Line::from(vec![
            Span::styled(
                format!("{} columns  {} cards", self.columns.len(), card_count),
                Style::default().fg(self.theme.secondary),
            ),
            Span::raw("  "),
            Span::styled(
                format!(
                    "{} repositories  {} annotations",
                    self.workspace_config.repositories.len(),
                    self.findings_count
                ),
                Style::default().fg(self.theme.secondary),
            ),
        ]);
        frame.render_widget(Paragraph::new(vec![title, summary]), layout.header);
    }

    fn render_board(&mut self, frame: &mut Frame, layout: &BoardLayout) {
        let goto_labels: HashMap<_, _> = self
            .goto_labels
            .as_ref()
            .map(|state| {
                state
                    .targets
                    .iter()
                    .map(|target| {
                        (
                            target.card_id,
                            (target.label.clone(), target.label.starts_with(&state.input)),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut visible_cards = Vec::new();

        for (slot, column_area) in layout.column_areas().into_iter().enumerate() {
            let column_index = self.viewport.first_column + slot;
            let Some(column) = self.columns.get(column_index) else {
                continue;
            };
            let focused = column_index == self.focused_column;
            let title = format!(" {} ({}) ", column.name, column.tasks.len());
            let block = Block::default()
                .title(truncate(
                    &title,
                    column_area.width.saturating_sub(2) as usize,
                ))
                .borders(Borders::ALL)
                .border_style(if focused {
                    self.theme.focus()
                } else {
                    Style::default().fg(self.theme.secondary)
                });
            let inner = block.inner(column_area);
            frame.render_widget(block, column_area);

            if column.tasks.is_empty() {
                frame.render_widget(
                    Paragraph::new("No cards")
                        .style(Style::default().fg(self.theme.secondary))
                        .alignment(Alignment::Center),
                    inner,
                );
                continue;
            }

            let offset = self
                .viewport
                .card_offsets
                .get(column_index)
                .copied()
                .unwrap_or(0)
                .min(column.tasks.len().saturating_sub(1));
            let capacity = usize::from(inner.height / 2);
            for (visible_index, (task_index, task)) in column
                .tasks
                .iter()
                .enumerate()
                .skip(offset)
                .take(capacity)
                .enumerate()
            {
                visible_cards.push(task.id);
                let row = Rect {
                    x: inner.x,
                    y: inner.y + visible_index as u16 * 2,
                    width: inner.width,
                    height: 2,
                };
                let is_focused = focused && task_index == self.focused_task;
                let is_selected = self.selected_cards.contains(&task.id);
                let goto = goto_labels.get(&task.id);
                let marker = if is_focused {
                    ">"
                } else if is_selected {
                    "*"
                } else {
                    " "
                };
                let label = goto
                    .map(|(label, _)| format!("[{label}] "))
                    .unwrap_or_default();
                let title_width = row.width.saturating_sub(2) as usize;
                let title = truncate(&format!("{marker} {label}{}", task.title), title_width);
                let mut title_style = if is_focused || is_selected {
                    self.theme.focus()
                } else {
                    Style::default()
                };
                if goto.is_some_and(|(_, matches)| !matches) {
                    title_style = Style::default().fg(self.theme.secondary);
                }
                let priority_style = match task.priority {
                    TaskPriority::HIGH => Style::default().fg(self.theme.error),
                    TaskPriority::MEDIUM => Style::default().fg(self.theme.warning),
                    TaskPriority::LOW => Style::default().fg(self.theme.secondary),
                };
                frame.render_widget(
                    Paragraph::new(vec![
                        Line::styled(title, title_style),
                        Line::from(vec![
                            Span::raw("  "),
                            Span::styled(task.priority.value(), priority_style),
                            Span::styled("  LOCAL", Style::default().fg(self.theme.secondary)),
                        ]),
                    ]),
                    row,
                );
            }
        }
        self.visible_cards = visible_cards;
    }

    fn render_status(&self, frame: &mut Frame, layout: &BoardLayout) {
        let explicit = self
            .last_error
            .as_ref()
            .map(|error| StatusMessage::error(error.clone()))
            .or_else(|| self.status_message.clone());
        if let Some(message) = explicit {
            frame.render_widget(
                Paragraph::new(format!(
                    "{}  {}",
                    match message.level {
                        StatusLevel::Info => "INFO",
                        StatusLevel::Warn => "WARN",
                        StatusLevel::Error => "ERROR",
                    },
                    message.text
                ))
                .style(self.theme.status(message.level)),
                layout.status,
            );
            return;
        }

        let path = abbreviated_path(&self.workspace_root, (layout.status.width / 2) as usize);
        let column = self.columns.get(self.focused_column);
        let position = column
            .and_then(|column| {
                (!column.tasks.is_empty()).then(|| {
                    format!(
                        "card {}/{}",
                        self.focused_task.saturating_add(1),
                        column.tasks.len()
                    )
                })
            })
            .unwrap_or_else(|| "No cards".to_owned());
        let selected = if self.selected_cards.is_empty() {
            String::new()
        } else {
            format!("  {} selected", self.selected_cards.len())
        };
        frame.render_widget(
            Paragraph::new(format!(
                "{path}  |  column {}/{} {}  |  {position}{selected}",
                self.focused_column.saturating_add(1),
                self.columns.len(),
                column.map(|column| column.name.as_str()).unwrap_or("")
            ))
            .style(Style::default().fg(self.theme.secondary)),
            layout.status,
        );
    }

    fn render_footer(&self, frame: &mut Frame, layout: &BoardLayout) {
        let hints = if self.goto_labels.is_some() {
            "Type a two-letter card label  Esc cancel".to_owned()
        } else if self.key_context() == KeyContext::Board && layout.footer.width <= 100 {
            "n new  e edit  H/L cards  [/] cols  m move  Space select  g goto  ? help  q quit"
                .to_owned()
        } else {
            keymap::footer_hints(self.key_context())
        };
        frame.render_widget(
            Paragraph::new(truncate(&hints, layout.footer.width as usize))
                .style(Style::default().fg(self.theme.secondary)),
            layout.footer,
        );
    }

    fn modal_block(&self, title: impl Into<Line<'static>>) -> Block<'static> {
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(self.theme.focus())
            .padding(Padding::horizontal(1))
    }

    fn render_detail(&self, frame: &mut Frame, area: Rect, clear: bool) {
        let Some(detail) = self.detail else {
            return;
        };
        let Some((column_index, task_index)) = self.locate_card(detail.card_id) else {
            return;
        };
        let task = &self.columns[column_index].tasks[task_index];
        if clear {
            frame.render_widget(Clear, area);
        }
        let block = self.modal_block(" Details ");
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let priority_style = match task.priority {
            TaskPriority::HIGH => Style::default().fg(self.theme.error),
            TaskPriority::MEDIUM => Style::default().fg(self.theme.warning),
            TaskPriority::LOW => Style::default().fg(self.theme.secondary),
        };
        let [
            title_area,
            metadata_area,
            location_area,
            description_title,
            description_area,
            actions,
        ] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(inner);
        frame.render_widget(
            Paragraph::new(task.title.as_str())
                .style(Style::default().add_modifier(Modifier::BOLD)),
            title_area,
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(task.priority.value(), priority_style),
                Span::styled("  LOCAL", Style::default().fg(self.theme.secondary)),
            ])),
            metadata_area,
        );
        frame.render_widget(
            Paragraph::new(format!(
                "{}  |  card {}/{}",
                self.columns[column_index].name,
                task_index + 1,
                self.columns[column_index].tasks.len()
            ))
            .style(Style::default().fg(self.theme.secondary)),
            location_area,
        );
        frame.render_widget(
            Paragraph::new("Description").style(self.theme.focus()),
            description_title,
        );
        frame.render_widget(
            Paragraph::new(task.description.as_str())
                .scroll((detail.scroll, 0))
                .wrap(Wrap { trim: false }),
            description_area,
        );
        frame.render_widget(
            Paragraph::new("e edit  d done  x archive")
                .style(Style::default().fg(self.theme.secondary)),
            actions,
        );
    }

    fn render_confirmation(&self, frame: &mut Frame, area: Rect) {
        let Some(confirmation) = &self.confirmation else {
            return;
        };
        let (title, detail) = match &confirmation.action {
            Confirmation::Archive { card_ids } => (
                format!(" Archive {} card(s)? ", card_ids.len()),
                card_ids
                    .iter()
                    .filter_map(|card_id| {
                        self.locate_card(*card_id)
                            .map(|(column, task)| self.columns[column].tasks[task].title.clone())
                    })
                    .take(3)
                    .collect::<Vec<_>>()
                    .join("  |  "),
            ),
            Confirmation::DeleteColumn { column_index } => (
                format!(" Delete column '{}'? ", self.columns[*column_index].name),
                "The shared workspace configuration will be updated.".to_owned(),
            ),
        };
        let popup = modal_area(area, 64, 7);
        frame.render_widget(Clear, popup);
        let block = self.modal_block(title);
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        let [detail_area, choice_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(inner);
        frame.render_widget(
            Paragraph::new(detail)
                .wrap(Wrap { trim: true })
                .alignment(Alignment::Center),
            detail_area,
        );
        let cancel_style = if !confirmation.confirm_selected {
            self.theme.focus().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let confirm_style = if confirmation.confirm_selected {
            self.theme.focus().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" Cancel ", cancel_style),
                Span::styled(" / ", Style::default().fg(self.theme.secondary)),
                Span::styled(" Confirm ", confirm_style),
            ]))
            .alignment(Alignment::Center),
            choice_area,
        );
    }

    fn render_move_picker(&self, frame: &mut Frame, area: Rect) {
        let Some(picker) = &self.move_picker else {
            return;
        };
        let popup = modal_area(area, 48, (self.columns.len() as u16 + 2).min(16));
        let items = self
            .columns
            .iter()
            .map(|column| ListItem::new(format!("{}  ({})", column.name, column.tasks.len())))
            .collect::<Vec<_>>();
        let mut state = ListState::default().with_selected(Some(picker.selected_column));
        frame.render_widget(Clear, popup);
        frame.render_stateful_widget(
            List::new(items)
                .block(self.modal_block(" Move cards to "))
                .highlight_style(self.theme.focus())
                .highlight_symbol("> "),
            popup,
            &mut state,
        );
    }

    fn render_goto_menu(&self, frame: &mut Frame, area: Rect) {
        if !self.goto_pending {
            return;
        }
        let popup = modal_area(area, 46, 7);
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Paragraph::new(
                "gw  visible card\ngg  first card    ge  last card\ngh  first column  gl  last column",
            )
            .block(self.modal_block(" Goto "))
            .wrap(Wrap { trim: false }),
            popup,
        );
    }

    fn render_help(&self, frame: &mut Frame, area: Rect) {
        if !self.help_open {
            return;
        }
        let popup = modal_area(
            area,
            area.width.saturating_mul(3) / 4,
            area.height.saturating_sub(2),
        );
        let lines = keymap::help_lines();
        let max_scroll = lines
            .len()
            .saturating_sub(popup.height.saturating_sub(2) as usize);
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Paragraph::new(lines.join("\n"))
                .scroll((self.help_scroll.min(max_scroll) as u16, 0))
                .block(self.modal_block(" Keyboard shortcuts "))
                .wrap(Wrap { trim: false }),
            popup,
        );
    }

    fn render_column_form(&self, frame: &mut Frame, area: Rect) {
        let Some(form) = &self.create_column_popup else {
            return;
        };
        let popup = modal_area(area, 52, 7);
        frame.render_widget(Clear, popup);
        let block = self.modal_block(" New column ");
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        let [input_area, error_area, action_area] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(inner);
        let input_block = Block::bordered()
            .title(" Name ")
            .border_style(self.theme.focus());
        let scroll = form
            .input
            .visual_scroll(input_area.width.saturating_sub(3) as usize);
        frame.render_widget(
            Paragraph::new(form.input.value())
                .scroll((0, scroll as u16))
                .block(input_block),
            input_area,
        );
        if let Some(error) = &form.error {
            frame.render_widget(
                Paragraph::new(error.as_str()).style(Style::default().fg(self.theme.error)),
                error_area,
            );
        }
        frame.render_widget(
            Paragraph::new("Ctrl+S create  Esc cancel")
                .style(Style::default().fg(self.theme.secondary)),
            action_area,
        );
        let cursor = form.input.visual_cursor().max(scroll) - scroll + 1;
        frame.set_cursor_position((input_area.x + cursor as u16, input_area.y + 1));
    }

    fn render_task_form(&mut self, frame: &mut Frame, area: Rect) {
        let Some(form) = &mut self.task_form else {
            return;
        };
        let popup = modal_area(
            area,
            area.width.saturating_sub(8).min(80),
            area.height.saturating_sub(2).min(18),
        );
        frame.render_widget(Clear, popup);
        let title = match form.mode {
            TaskFormMode::Create => " New card ",
            TaskFormMode::Edit(_) => " Edit card ",
        };
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(self.theme.focus())
            .padding(Padding::horizontal(1));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        let [
            title_area,
            description_area,
            priority_area,
            error_area,
            actions_area,
        ] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(inner);

        let title_active = matches!(self.active_input, Some(InputType::CreatingTaskTitle));
        let title_scroll = form
            .title
            .visual_scroll(title_area.width.saturating_sub(3) as usize);
        frame.render_widget(
            Paragraph::new(form.title.value())
                .scroll((0, title_scroll as u16))
                .block(
                    Block::bordered()
                        .title(" Title ")
                        .border_style(if title_active {
                            self.theme.focus()
                        } else {
                            Style::default().fg(self.theme.secondary)
                        }),
                ),
            title_area,
        );
        if title_active {
            let cursor = form.title.visual_cursor().max(title_scroll) - title_scroll + 1;
            frame.set_cursor_position((title_area.x + cursor as u16, title_area.y + 1));
        }

        let description_active =
            matches!(self.active_input, Some(InputType::CreatingTaskDescription));
        form.description
            .set_block(Block::bordered().title(" Description ").border_style(
                if description_active {
                    self.theme.focus()
                } else {
                    Style::default().fg(self.theme.secondary)
                },
            ));
        form.description.set_cursor_style(if description_active {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        });
        frame.render_widget(&form.description, description_area);

        let priority_active = matches!(self.active_input, Some(InputType::CreatingTaskPriority));
        let priority_block =
            Block::bordered()
                .title(" Priority ")
                .border_style(if priority_active {
                    self.theme.focus()
                } else {
                    Style::default().fg(self.theme.secondary)
                });
        let priority_inner = priority_block.inner(priority_area);
        frame.render_widget(priority_block, priority_area);
        let priority_line = [TaskPriority::LOW, TaskPriority::MEDIUM, TaskPriority::HIGH]
            .into_iter()
            .flat_map(|priority| {
                let style = if form.priority == priority {
                    match priority {
                        TaskPriority::HIGH => Style::default()
                            .fg(self.theme.error)
                            .add_modifier(Modifier::REVERSED),
                        TaskPriority::MEDIUM => Style::default()
                            .fg(self.theme.warning)
                            .add_modifier(Modifier::REVERSED),
                        TaskPriority::LOW => Style::default()
                            .fg(self.theme.focus)
                            .add_modifier(Modifier::REVERSED),
                    }
                } else {
                    Style::default().fg(self.theme.secondary)
                };
                [
                    Span::styled(format!(" {} ", priority.value()), style),
                    Span::raw("  "),
                ]
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(Line::from(priority_line)).alignment(Alignment::Center),
            priority_inner,
        );
        if let Some(error) = &form.error {
            frame.render_widget(
                Paragraph::new(error.as_str()).style(Style::default().fg(self.theme.error)),
                error_area,
            );
        }
        frame.render_widget(
            Paragraph::new("Tab next field  ←/→ priority  Ctrl+S save  Esc cancel")
                .style(Style::default().fg(self.theme.secondary)),
            actions_area,
        );
    }

    fn delete_column(&mut self, column_index: usize) {
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

    fn scroll_to_focused_column(&mut self) {
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

#[cfg(test)]
mod tests {
    use std::fs;

    use crossterm::event::KeyModifiers;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::storage::AppPaths;
    use crate::workspace::{WorkspaceInitializer, WorkspaceManager};

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("kodeck-app-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn test_app(temporary: &TestDirectory) -> App<'static> {
        let root = temporary.0.join("project");
        fs::create_dir_all(&root).unwrap();
        let paths = AppPaths::new(temporary.0.join("config"), temporary.0.join("data"));
        let context = WorkspaceInitializer::new(paths)
            .initialize(&root, "Test Board")
            .unwrap();
        App::from_workspace(context, 0)
    }

    fn add_task(app: &mut App<'_>, column: usize, title: &str) -> CardId {
        let order = app.columns[column].tasks.len();
        let task = Task::new(
            title.to_owned(),
            format!("Description for {title}"),
            TaskPriority::MEDIUM,
            order,
        );
        let id = task.id;
        app.columns[column].tasks.push(task);
        id
    }

    fn press(app: &mut App<'_>, code: KeyCode) {
        assert!(!app.handle_key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    fn press_ctrl(app: &mut App<'_>, character: char) {
        assert!(!app.handle_key(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::CONTROL,
        )));
    }

    #[test]
    fn board_mutations_are_reloaded_from_workspace_storage() {
        let temporary = TestDirectory::new();
        let root = temporary.0.join("project");
        fs::create_dir_all(&root).unwrap();
        let paths = AppPaths::new(temporary.0.join("config"), temporary.0.join("data"));
        let context = WorkspaceInitializer::new(paths.clone())
            .initialize(&root, "Test Board")
            .unwrap();
        let mut app = App::from_workspace(context, 0);

        app.focused_column = app
            .columns
            .iter()
            .position(|column| column.id.as_str() == "todo")
            .unwrap();
        app.columns[app.focused_column].tasks.push(Task::new(
            "Persist me".to_owned(),
            "Stored outside Git".to_owned(),
            TaskPriority::HIGH,
            0,
        ));
        app.persist_cards();
        app.move_targets_relative(1);
        app.create_column("Blocked".to_owned());
        drop(app);

        let reloaded = WorkspaceManager::new(paths).open_root(&root).unwrap();
        assert!(
            reloaded
                .config
                .columns
                .iter()
                .any(|column| column.id.as_str() == "blocked")
        );
        let card = &reloaded.private_state.cards.cards[0];
        assert_eq!(card.title, "Persist me");
        assert_eq!(card.column_id.as_str(), "in-progress");
    }

    #[test]
    fn empty_workspace_board_renders_without_panicking() {
        let temporary = TestDirectory::new();
        let root = temporary.0.join("project");
        fs::create_dir_all(&root).unwrap();
        let paths = AppPaths::new(temporary.0.join("config"), temporary.0.join("data"));
        let context = WorkspaceInitializer::new(paths)
            .initialize(&root, "Empty Board")
            .unwrap();
        let mut app = App::from_workspace(context, 0);
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| app.render(frame)).unwrap();

        assert!(terminal.backend().to_string().contains("Empty Board"));
    }

    #[test]
    fn vim_navigation_and_aliases_stop_at_board_boundaries() {
        let temporary = TestDirectory::new();
        let mut app = test_app(&temporary);
        add_task(&mut app, 0, "First");
        add_task(&mut app, 0, "Second");

        press(&mut app, KeyCode::Char('h'));
        press(&mut app, KeyCode::Char('k'));
        assert_eq!((app.focused_column, app.focused_task), (0, 0));

        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Down);
        assert_eq!(app.focused_task, 1);

        press(&mut app, KeyCode::Tab);
        assert_eq!(app.focused_column, 1);
        for _ in 0..10 {
            press(&mut app, KeyCode::Char('l'));
        }
        assert_eq!(app.focused_column, app.columns.len() - 1);
    }

    #[test]
    fn column_reordering_preserves_focus_selection_offsets_and_persists() {
        let temporary = TestDirectory::new();
        let root = temporary.0.join("project");
        fs::create_dir_all(&root).unwrap();
        let paths = AppPaths::new(temporary.0.join("config"), temporary.0.join("data"));
        let context = WorkspaceInitializer::new(paths.clone())
            .initialize(&root, "Reordered Board")
            .unwrap();
        let mut app = App::from_workspace(context, 0);
        let first = add_task(&mut app, 1, "Focused");
        let second = add_task(&mut app, 1, "Keep order");
        let selected = add_task(&mut app, 2, "Selected");
        app.focused_column = 1;
        app.focused_task = 0;
        app.selected_cards.insert(selected);
        app.viewport.card_offsets[0] = 1;
        app.viewport.card_offsets[1] = 3;
        app.persist_cards();
        let original_columns: Vec<_> = app.columns.iter().map(|column| column.id.clone()).collect();

        press(&mut app, KeyCode::Char('['));

        assert_eq!(app.focused_column, 0);
        assert_eq!(app.focused_card_id(), Some(first));
        assert!(app.selected_cards.contains(&selected));
        assert_eq!(app.viewport.card_offsets[0], 3);
        assert_eq!(app.viewport.card_offsets[1], 1);
        assert_eq!(app.columns[0].id, original_columns[1]);
        assert_eq!(app.columns[1].id, original_columns[0]);
        assert_eq!(
            app.columns[0]
                .tasks
                .iter()
                .map(|task| task.id)
                .collect::<Vec<_>>(),
            vec![first, second]
        );
        assert_eq!(
            app.status_message
                .as_ref()
                .map(|message| message.text.as_str()),
            Some("Column moved left")
        );

        let reloaded = WorkspaceManager::new(paths).open_root(&root).unwrap();
        let persisted_columns: Vec<_> = reloaded
            .config
            .columns
            .iter()
            .map(|column| column.id.clone())
            .collect();
        assert_eq!(
            persisted_columns,
            app.columns
                .iter()
                .map(|column| column.id.clone())
                .collect::<Vec<_>>()
        );
        let stored_card = reloaded
            .private_state
            .cards
            .cards
            .iter()
            .find(|card| card.id == first)
            .unwrap();
        assert_eq!(stored_card.column_id, original_columns[1]);

        for width in [80, 160] {
            let backend = TestBackend::new(width, 24);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal.draw(|frame| app.render(frame)).unwrap();
            let layout = BoardLayout::compute(Rect::new(0, 0, width, 24), app.columns.len(), false);
            assert!(app.focused_column >= app.viewport.first_column);
            assert!(app.focused_column < app.viewport.first_column + layout.visible_columns);
        }
    }

    #[test]
    fn column_reordering_respects_boundaries_and_works_from_details() {
        let temporary = TestDirectory::new();
        let mut app = test_app(&temporary);
        let card_id = add_task(&mut app, 0, "Follow the column");

        press(&mut app, KeyCode::Char('['));
        assert_eq!(app.focused_column, 0);
        assert_eq!(
            app.status_message
                .as_ref()
                .map(|message| message.text.as_str()),
            Some("Column cannot move further")
        );

        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char(']'));
        assert_eq!(app.focused_column, 1);
        assert_eq!(app.focused_card_id(), Some(card_id));
        assert_eq!(app.detail.map(|detail| detail.card_id), Some(card_id));

        app.focused_column = app.columns.len() - 1;
        press(&mut app, KeyCode::Char(']'));
        assert_eq!(app.focused_column, app.columns.len() - 1);
        assert_eq!(
            app.status_message
                .as_ref()
                .map(|message| message.text.as_str()),
            Some("Column cannot move further")
        );
    }

    #[test]
    fn relative_move_applies_to_each_selected_card_and_clears_selection() {
        let temporary = TestDirectory::new();
        let mut app = test_app(&temporary);
        let first = add_task(&mut app, 0, "Inbox");
        let second = add_task(&mut app, 1, "Todo");
        let last_column = app.columns.len() - 1;
        let boundary = add_task(&mut app, last_column, "Already done");
        app.selected_cards = HashSet::from([first, second, boundary]);

        press(&mut app, KeyCode::Char('L'));

        assert_eq!(app.locate_card(first).unwrap().0, 1);
        assert_eq!(app.locate_card(second).unwrap().0, 2);
        assert_eq!(app.locate_card(boundary).unwrap().0, app.columns.len() - 1);
        assert!(app.selected_cards.is_empty());
    }

    #[test]
    fn move_picker_sends_a_multi_column_selection_to_one_destination() {
        let temporary = TestDirectory::new();
        let mut app = test_app(&temporary);
        let first = add_task(&mut app, 0, "Inbox");
        let second = add_task(&mut app, 1, "Todo");
        app.selected_cards = HashSet::from([first, second]);
        app.focused_column = 1;

        press(&mut app, KeyCode::Char('m'));
        assert!(app.move_picker.is_some());
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Char('j'));
        let destination = app.move_picker.as_ref().unwrap().selected_column;
        press(&mut app, KeyCode::Enter);

        assert_eq!(app.locate_card(first).unwrap().0, destination);
        assert_eq!(app.locate_card(second).unwrap().0, destination);
        assert!(app.move_picker.is_none());
        assert!(app.selected_cards.is_empty());
    }

    #[test]
    fn archive_requires_confirmation_and_y_accepts_it() {
        let temporary = TestDirectory::new();
        let mut app = test_app(&temporary);
        let card_id = add_task(&mut app, 0, "Keep or archive");

        press(&mut app, KeyCode::Char('x'));
        assert!(app.confirmation.is_some());
        press(&mut app, KeyCode::Enter);
        assert!(app.locate_card(card_id).is_some());

        press(&mut app, KeyCode::Char('x'));
        press(&mut app, KeyCode::Char('y'));
        assert!(app.locate_card(card_id).is_none());
        assert_eq!(app.archived_tasks.last().unwrap().task.id, card_id);
    }

    #[test]
    fn goto_sequences_and_visible_labels_move_focus() {
        let temporary = TestDirectory::new();
        let mut app = test_app(&temporary);
        let first = add_task(&mut app, 0, "First target");
        let last_column = app.columns.len() - 1;
        let last = add_task(&mut app, last_column, "Last target");
        app.focus_card(last);

        press(&mut app, KeyCode::Char('g'));
        assert!(app.goto_pending);
        press(&mut app, KeyCode::Char('g'));
        assert_eq!(app.focused_card_id(), Some(first));

        press(&mut app, KeyCode::Char('g'));
        press(&mut app, KeyCode::Char('e'));
        assert_eq!(app.focused_card_id(), Some(last));
        press(&mut app, KeyCode::Char('g'));
        press(&mut app, KeyCode::Char('h'));
        assert_eq!(app.focused_column, 0);
        press(&mut app, KeyCode::Char('g'));
        press(&mut app, KeyCode::Char('l'));
        assert_eq!(app.focused_column, last_column);

        let backend = TestBackend::new(220, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.render(frame)).unwrap();
        press(&mut app, KeyCode::Char('g'));
        press(&mut app, KeyCode::Char('w'));
        let target = app
            .goto_labels
            .as_ref()
            .unwrap()
            .targets
            .last()
            .unwrap()
            .clone();
        for character in target.label.chars() {
            press(&mut app, KeyCode::Char(character));
        }
        assert_eq!(app.focused_card_id(), Some(target.card_id));
        assert!(app.goto_labels.is_none());
    }

    #[test]
    fn ctrl_s_saves_card_forms_and_escape_discards_them() {
        let temporary = TestDirectory::new();
        let mut app = test_app(&temporary);

        press(&mut app, KeyCode::Char('n'));
        for character in "New card".chars() {
            press(&mut app, KeyCode::Char(character));
        }
        press(&mut app, KeyCode::Enter);
        for character in "Useful description".chars() {
            press(&mut app, KeyCode::Char(character));
        }
        press_ctrl(&mut app, 's');

        assert!(app.task_form.is_none());
        assert_eq!(app.columns[0].tasks[0].title, "New card");

        press(&mut app, KeyCode::Char('n'));
        press(&mut app, KeyCode::Char('x'));
        press(&mut app, KeyCode::Esc);
        assert!(app.task_form.is_none());
        assert_eq!(app.columns[0].tasks.len(), 1);
    }

    #[test]
    fn ctrl_s_creates_a_column_and_x_deletes_it_after_confirmation() {
        let temporary = TestDirectory::new();
        let mut app = test_app(&temporary);
        let original_count = app.columns.len();

        press(&mut app, KeyCode::Char('N'));
        for character in "Blocked".chars() {
            press(&mut app, KeyCode::Char(character));
        }
        press_ctrl(&mut app, 's');
        assert_eq!(app.columns.len(), original_count + 1);
        assert_eq!(app.columns.last().unwrap().name, "Blocked");

        app.focused_column = app.columns.len() - 1;
        press(&mut app, KeyCode::Char('X'));
        press(&mut app, KeyCode::Char('h'));
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.columns.len(), original_count);
        assert!(app.columns.iter().all(|column| column.name != "Blocked"));
    }

    #[test]
    fn help_goto_and_confirmation_render_on_a_compact_terminal() {
        let temporary = TestDirectory::new();
        let mut app = test_app(&temporary);
        add_task(&mut app, 0, "Compact");
        let backend = TestBackend::new(50, 16);
        let mut terminal = Terminal::new(backend).unwrap();

        press(&mut app, KeyCode::Char('?'));
        terminal.draw(|frame| app.render(frame)).unwrap();
        assert!(
            terminal
                .backend()
                .to_string()
                .contains("Keyboard shortcuts")
        );

        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('g'));
        terminal.draw(|frame| app.render(frame)).unwrap();
        assert!(terminal.backend().to_string().contains("visible card"));

        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('x'));
        terminal.draw(|frame| app.render(frame)).unwrap();
        let rendered = terminal.backend().to_string();
        assert!(rendered.contains("Archive 1 card"));
        assert!(rendered.contains("Compact"));
    }

    #[test]
    fn board_footer_keeps_help_and_quit_visible_at_eighty_columns() {
        let temporary = TestDirectory::new();
        let mut app = test_app(&temporary);
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| app.render(frame)).unwrap();

        let rendered = terminal.backend().to_string();
        assert!(rendered.contains("? help"));
        assert!(rendered.contains("q quit"));
    }

    #[test]
    fn complete_column_viewport_tracks_focus_at_supported_widths() {
        let temporary = TestDirectory::new();
        let mut app = test_app(&temporary);
        app.create_column("Blocked".to_owned());
        app.create_column("Ready".to_owned());
        app.create_column("Released".to_owned());
        for column in 0..app.columns.len() {
            add_task(&mut app, column, &format!("Card in column {column}"));
        }
        app.focus_column(app.columns.len() - 1);

        for width in [40, 60, 80, 120, 160] {
            let backend = TestBackend::new(width, 24);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal.draw(|frame| app.render(frame)).unwrap();
            let layout = BoardLayout::compute(Rect::new(0, 0, width, 24), app.columns.len(), false);
            if width < 50 {
                assert!(
                    terminal
                        .backend()
                        .to_string()
                        .contains("Terminal too small")
                );
            } else {
                assert!(app.focused_column >= app.viewport.first_column);
                assert!(app.focused_column < app.viewport.first_column + layout.visible_columns);
                assert_eq!(app.visible_cards.len(), layout.visible_columns);
            }
        }
    }

    #[test]
    fn vertical_viewport_never_slices_two_line_cards() {
        let temporary = TestDirectory::new();
        let mut app = test_app(&temporary);
        let mut last = None;
        for index in 0..12 {
            last = Some(add_task(
                &mut app,
                0,
                &format!("A long card title number {index} that must truncate"),
            ));
        }
        app.focused_task = 11;
        let backend = TestBackend::new(80, 16);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| app.render(frame)).unwrap();

        assert_eq!(app.viewport.card_offsets[0], 7);
        assert_eq!(app.visible_cards.len(), 5);
        assert!(app.visible_cards.contains(&last.unwrap()));
        assert!(terminal.backend().to_string().contains('…'));
    }

    #[test]
    fn details_follow_board_navigation_and_scroll_independently() {
        let temporary = TestDirectory::new();
        let mut app = test_app(&temporary);
        let first = add_task(&mut app, 0, "First");
        let second = add_task(&mut app, 0, "Second");
        app.columns[0].tasks[1].description = (1..=30)
            .map(|line| format!("Description line {line}"))
            .collect::<Vec<_>>()
            .join("\n");

        press(&mut app, KeyCode::Enter);
        assert_eq!(app.detail.unwrap().card_id, first);
        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.detail.unwrap().card_id, second);
        press_ctrl(&mut app, 'd');
        assert_eq!(app.detail.unwrap().scroll, 4);

        for width in [80, 120, 160] {
            let backend = TestBackend::new(width, 24);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal.draw(|frame| app.render(frame)).unwrap();
            let rendered = terminal.backend().to_string();
            assert!(rendered.contains("Details"));
            assert!(rendered.contains("Description line 5"));
        }
        press(&mut app, KeyCode::Esc);
        assert!(app.detail.is_none());
    }

    #[test]
    fn unified_task_form_reports_inline_validation_errors() {
        let temporary = TestDirectory::new();
        let mut app = test_app(&temporary);

        press(&mut app, KeyCode::Char('n'));
        press_ctrl(&mut app, 's');
        assert_eq!(
            app.task_form.as_ref().unwrap().error.as_deref(),
            Some("Title is required")
        );
        for character in "Only a title".chars() {
            press(&mut app, KeyCode::Char(character));
        }
        press_ctrl(&mut app, 's');
        assert_eq!(
            app.task_form.as_ref().unwrap().error.as_deref(),
            Some("Description is required")
        );
        press(&mut app, KeyCode::Esc);
        assert!(app.task_form.is_none());
    }

    #[test]
    fn unified_edit_form_updates_the_card_identified_by_id_and_persists_it() {
        let temporary = TestDirectory::new();
        let mut app = test_app(&temporary);
        let card_id = add_task(&mut app, 0, "Before");
        app.focused_task = 0;

        press(&mut app, KeyCode::Char('e'));
        let form = app.task_form.as_mut().unwrap();
        assert_eq!(form.mode, TaskFormMode::Edit(card_id));
        form.title = tui_input::Input::from("After".to_owned());
        form.description = ratatui_textarea::TextArea::from(["Persisted description"]);
        press_ctrl(&mut app, 's');

        assert!(app.task_form.is_none());
        assert_eq!(app.columns[0].tasks[0].title, "After");
        let stored = app.private_store.load_cards().unwrap();
        let stored_card = stored.cards.iter().find(|card| card.id == card_id).unwrap();
        assert_eq!(stored_card.title, "After");
        assert_eq!(stored_card.description, "Persisted description");
    }
}
