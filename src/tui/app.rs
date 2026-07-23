use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use color_eyre::eyre::Result;
use crossterm::event;
use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, HorizontalAlignment, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{
    Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
};
use ratatui::{DefaultTerminal, Frame};
use ratatui_textarea::Input;
use tui_input::backend::crossterm::EventHandler;

use crate::domain::{
    CardsFile, Column, ColumnConfig, ColumnId, LocalCard, Task, TaskPriority, UiStateFile,
    WorkspaceConfig,
};
use crate::workspace::{PrivateWorkspaceStore, SharedWorkspaceStore, WorkspaceContext};

use super::popups::{CreateColumnPopup, CreatingTaskPopup, DeleteColumnPopup, DisplayingTaskPopup};
use super::state::{InputMode, InputType, Popup, VisualSelectionDirection};

const COLUMNS_GAP: u16 = 4;

struct ArchivedTask {
    column_id: ColumnId,
    task: Task,
}

pub struct App<'a> {
    workspace_root: PathBuf,
    workspace_config: WorkspaceConfig,
    shared_store: SharedWorkspaceStore,
    private_store: PrivateWorkspaceStore,
    ui_state: UiStateFile,
    warnings: Vec<String>,
    findings_count: usize,
    last_error: Option<String>,
    input_mode: InputMode,
    active_input: Option<InputType>,
    scroll_x: usize,
    scroll_x_state: ScrollbarState,
    area_width: u16,
    visual_selection_active: bool,
    visual_selection_direction: Option<VisualSelectionDirection>,
    // COLUMNS MANAGEMENT
    columns: Vec<Column>,
    focused_column: usize,
    delete_focused_column: usize,
    create_column_popup: Option<CreateColumnPopup>,
    delete_column_popup: Option<DeleteColumnPopup>,
    visual_selected_columns_indexes: Vec<usize>,
    visual_anchor_column: usize,
    // TASKS MANAGEMENT
    focused_task: usize,
    creating_task_popup: Option<CreatingTaskPopup<'a>>,
    displaying_task_popup: Option<DisplayingTaskPopup>,
    edit_task_popup: Option<CreatingTaskPopup<'a>>,
    archived_tasks: Vec<ArchivedTask>,
    visual_selected_tasks_indexes: Vec<(usize, usize)>,
    visual_anchor_task: usize,
}

// TODO : Done on a task => what's the point ? define a done column ? hide them ?
// TODO : CLEAN BULK IN VISUAL AND ADD SUPPORT FOR SPACE IN NORMAL MODE
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

        Self {
            workspace_root: context.root,
            workspace_config: context.config,
            shared_store,
            private_store,
            ui_state,
            warnings,
            findings_count,
            last_error: None,
            input_mode: InputMode::Normal,
            active_input: None,
            columns,
            scroll_x: 0,
            scroll_x_state: ScrollbarState::new(5),
            area_width: 0,
            visual_selection_active: false,
            visual_selection_direction: None,
            focused_column,
            delete_focused_column: 0,
            focused_task,
            create_column_popup: None,
            delete_column_popup: None,
            visual_selected_columns_indexes: Vec::new(),
            visual_anchor_column: 0,
            creating_task_popup: None,
            displaying_task_popup: None,
            edit_task_popup: None,
            archived_tasks,
            visual_selected_tasks_indexes: Vec::new(),
            visual_anchor_task: 0,
        }
    }

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        loop {
            terminal.draw(|frame| self.render(frame))?;

            if let Some(key) = event::read()?.as_key_press_event() {
                match self.input_mode {
                    InputMode::Normal if self.displaying_task_popup.is_none() => match key.code {
                        KeyCode::Char('i') => self.input_mode = InputMode::Editing,
                        KeyCode::Char('v') => {
                            self.input_mode = InputMode::Visual;
                            self.visual_selection_active = true;
                            self.visual_anchor_column = self.focused_column;
                            self.visual_anchor_task = self.focused_task;
                            self.visual_selected_columns_indexes = vec![];
                            self.visual_selected_tasks_indexes = vec![];
                        }
                        KeyCode::Char('c') => {
                            if !self.is_not_bulk() {
                                continue;
                            }
                            self.input_mode = InputMode::Editing;
                            self.toggle_column_creation_popup();
                        }
                        KeyCode::Char('t') => {
                            if !self.is_not_bulk() {
                                continue;
                            }
                            self.input_mode = InputMode::Editing;
                            self.toggle_task_creation_popup();
                        }
                        KeyCode::Char('D') => {
                            if !self.is_not_bulk() {
                                continue;
                            }
                            self.delete_focused_column = self.focused_column;
                            self.open_popup(Popup::DeleteColumn);
                        }
                        //TODO : bulk move
                        KeyCode::Char('s') => {
                            if !self.is_not_bulk() {
                                continue;
                            }
                            self.send_task_to_next();
                        }
                        KeyCode::Char('S') => {
                            if !self.is_not_bulk() {
                                continue;
                            }
                            self.send_task_to_prev();
                        }
                        KeyCode::Char('e') => {
                            if !self.is_not_bulk() {
                                continue;
                            }
                            if self.columns[self.focused_column].tasks.is_empty() {
                                continue;
                            }
                            self.input_mode = InputMode::Editing;
                            self.open_popup(Popup::EditTask);
                        }
                        KeyCode::Char('E') => {}
                        KeyCode::Up => {
                            let column = &self.columns[self.focused_column];
                            if column.tasks.is_empty() {
                                continue;
                            }
                            self.focused_task =
                                self.focused_task.wrapping_sub(1) % column.tasks.len();
                            self.persist_ui_state();
                        }
                        KeyCode::Down => {
                            let column = &self.columns[self.focused_column];
                            if column.tasks.is_empty() {
                                continue;
                            }
                            self.focused_task =
                                self.focused_task.wrapping_add(1) % column.tasks.len();
                            self.persist_ui_state();
                        }
                        KeyCode::Backspace => {
                            let column = &mut self.columns[self.focused_column];
                            if column.tasks.is_empty() {
                                continue;
                            }

                            if self.focused_task >= column.tasks.len() {
                                continue;
                            }

                            let column_id = column.id.clone();
                            let task = column.tasks.remove(self.focused_task);
                            if column.tasks.is_empty() {
                                self.focused_task = 0;
                            } else {
                                self.focused_task =
                                    self.focused_task.wrapping_sub(1) % column.tasks.len();
                            }
                            self.archived_tasks.push(ArchivedTask { column_id, task });
                            self.persist_cards();
                        }
                        KeyCode::Tab => {
                            if let Some(popup) = &mut self.delete_column_popup {
                                popup.delete = !popup.delete;
                                continue;
                            }
                            self.focused_column =
                                self.focused_column.saturating_add(1) % self.columns.len();
                            self.focused_task = 0;
                            self.scroll_to_focused_column();
                            self.persist_ui_state();
                        }
                        KeyCode::BackTab => {
                            if let Some(popup) = &mut self.delete_column_popup {
                                popup.delete = !popup.delete;
                                continue;
                            }
                            self.focused_column = self
                                .focused_column
                                .checked_sub(1)
                                .unwrap_or(self.columns.len() - 1);
                            self.focused_task = 0;
                            self.scroll_to_focused_column();
                            self.persist_ui_state();
                        }
                        KeyCode::Char(' ') => {
                            let key = (self.focused_column, self.focused_task);
                            if self.visual_selected_tasks_indexes.contains(&key) {
                                self.visual_selected_tasks_indexes.retain(|&k| k != key);
                            } else {
                                self.visual_selected_tasks_indexes.push(key);
                                self.visual_selection_active = true;
                            }
                            if self.visual_selected_tasks_indexes.is_empty() {
                                self.visual_selection_active = false;
                            }
                        }
                        KeyCode::Enter => {
                            if !self.is_not_bulk() {
                                let mut by_column: std::collections::HashMap<usize, Vec<usize>> =
                                    HashMap::new();
                                for (col, task) in &self.visual_selected_tasks_indexes {
                                    by_column.entry(*col).or_default().push(*task);
                                }
                                for (col_idx, mut task_indexes) in by_column {
                                    task_indexes.sort();
                                    for task_idx in task_indexes.iter().rev() {
                                        let task = self.columns[col_idx].tasks.remove(*task_idx);
                                        self.columns[self.focused_column].tasks.push(task);
                                    }
                                }
                                self.visual_selected_tasks_indexes.clear();
                                self.visual_selection_active = false;
                                self.persist_cards();
                                continue;
                            }
                            if let Some(popup) = &mut self.delete_column_popup {
                                if popup.delete {
                                    self.delete_column(self.delete_focused_column);
                                }
                                self.close_popup(Popup::DeleteColumn);
                                continue;
                            }
                            if self.columns[self.focused_column].tasks.is_empty() {
                                continue;
                            }
                            self.toggle_task_displaying_popup(
                                self.columns[self.focused_column].tasks[self.focused_task].clone(),
                            );
                        }
                        KeyCode::Esc => {
                            if self.delete_column_popup.is_some() {
                                self.close_popup(Popup::DeleteColumn);
                                continue;
                            }
                        }
                        KeyCode::Char('q') => {
                            self.persist_ui_state();
                            return Ok(());
                        }
                        _ => {}
                    },
                    InputMode::Normal => match key.code {
                        KeyCode::Esc => {
                            self.toggle_task_displaying_popup(
                                self.columns[self.focused_column].tasks[self.focused_task].clone(),
                            );
                        }
                        KeyCode::Char('d') => {
                            if let Some(done_column) = self
                                .columns
                                .iter()
                                .position(|column| column.id.as_str() == "done")
                                && done_column != self.focused_column
                            {
                                self.send_task_to_column(
                                    self.focused_task,
                                    self.focused_column,
                                    done_column,
                                );
                                self.focused_column = done_column;
                                self.focused_task =
                                    self.columns[done_column].tasks.len().saturating_sub(1);
                            }
                            self.displaying_task_popup = None;
                            self.persist_ui_state();
                        }
                        _ => {}
                    },
                    InputMode::Editing if key.kind == KeyEventKind::Press => match key.code {
                        KeyCode::Esc => {
                            match self.active_input {
                                Some(InputType::CreatingColumn) => {
                                    self.toggle_column_creation_popup();
                                }
                                Some(InputType::CreatingTaskTitle)
                                | Some(InputType::CreatingTaskDescription)
                                | Some(InputType::CreatingTaskPriority) => {
                                    if self.creating_task_popup.is_some() {
                                        self.toggle_task_creation_popup();
                                    } else if self.edit_task_popup.is_some() {
                                        self.close_popup(Popup::EditTask);
                                    }
                                }
                                None => {}
                            }
                            self.active_input = None;
                            self.input_mode = InputMode::Normal;
                        }
                        KeyCode::Char(_) => {
                            let event = crossterm::event::Event::Key(key);
                            match &mut self.active_input {
                                Some(InputType::CreatingColumn) => {
                                    if let Some(popup) = &mut self.create_column_popup {
                                        popup.input.handle_event(&event);
                                    }
                                }
                                Some(InputType::CreatingTaskTitle) => {
                                    if let Some(popup) = &mut self.creating_task_popup {
                                        popup.title.handle_event(&event);
                                    } else if let Some(popup) = &mut self.edit_task_popup {
                                        popup.title.handle_event(&event);
                                    }
                                }
                                Some(InputType::CreatingTaskDescription) => {
                                    if let Some(popup) = &mut self.creating_task_popup {
                                        let textarea_event: Input = event::Event::Key(key).into();
                                        popup.description.input(textarea_event);
                                    } else if let Some(popup) = &mut self.edit_task_popup {
                                        let textarea_event: Input = event::Event::Key(key).into();
                                        popup.description.input(textarea_event);
                                    }
                                }
                                _ => {}
                            }
                        }
                        KeyCode::Backspace => {
                            let event = event::Event::Key(key);
                            match &mut self.active_input {
                                Some(InputType::CreatingColumn) => {
                                    if let Some(popup) = &mut self.create_column_popup {
                                        popup.input.handle_event(&event);
                                    }
                                }
                                Some(InputType::CreatingTaskTitle) => {
                                    if let Some(popup) = &mut self.creating_task_popup {
                                        popup.title.handle_event(&event);
                                    } else if let Some(popup) = &mut self.edit_task_popup {
                                        popup.title.handle_event(&event);
                                    }
                                }
                                Some(InputType::CreatingTaskDescription) => {
                                    if let Some(popup) = &mut self.creating_task_popup {
                                        let textarea_event: Input = event::Event::Key(key).into();
                                        popup.description.input(textarea_event);
                                    } else if let Some(popup) = &mut self.edit_task_popup {
                                        let textarea_event: Input = event::Event::Key(key).into();
                                        popup.description.input(textarea_event);
                                    }
                                }
                                _ => {}
                            }
                        }
                        KeyCode::Enter => match self.active_input {
                            Some(InputType::CreatingColumn) => {
                                if let Some(name) = self
                                    .create_column_popup
                                    .as_ref()
                                    .map(|popup| popup.input.value().to_owned())
                                {
                                    self.create_column(name);
                                }
                                self.create_column_popup = None;
                                self.active_input = None;
                                self.input_mode = InputMode::Normal;
                            }
                            Some(InputType::CreatingTaskTitle)
                            | Some(InputType::CreatingTaskPriority) => {
                                if self.edit_task_popup.is_some() {
                                    self.safe_edit_task();
                                    continue;
                                }
                                if self.safe_create_task() {
                                    continue;
                                }
                            }
                            Some(InputType::CreatingTaskDescription) => {
                                if key
                                    .modifiers
                                    .contains(crossterm::event::KeyModifiers::SHIFT)
                                {
                                    if self.edit_task_popup.is_some() {
                                        self.safe_edit_task();
                                        continue;
                                    }
                                    if self.safe_create_task() {
                                        continue;
                                    }
                                } else {
                                    if let Some(popup) = &mut self.creating_task_popup {
                                        let textarea_event: Input = event::Event::Key(key).into();
                                        popup.description.input(textarea_event);
                                    } else if let Some(popup) = &mut self.edit_task_popup {
                                        let textarea_event: Input = event::Event::Key(key).into();

                                        popup.description.input(textarea_event);
                                    }
                                }
                            }
                            None => {}
                        },
                        KeyCode::Tab => match self.active_input {
                            Some(InputType::CreatingTaskTitle) => {
                                self.active_input = Some(InputType::CreatingTaskDescription);
                            }
                            Some(InputType::CreatingTaskDescription) => {
                                self.active_input = Some(InputType::CreatingTaskPriority);
                            }
                            Some(InputType::CreatingTaskPriority) => {
                                self.active_input = Some(InputType::CreatingTaskTitle);
                            }
                            None => {}
                            _ => {}
                        },
                        KeyCode::BackTab => match self.active_input {
                            Some(InputType::CreatingTaskTitle) => {
                                self.active_input = Some(InputType::CreatingTaskPriority);
                            }
                            Some(InputType::CreatingTaskDescription) => {
                                self.active_input = Some(InputType::CreatingTaskTitle);
                            }
                            Some(InputType::CreatingTaskPriority) => {
                                self.active_input = Some(InputType::CreatingTaskDescription);
                            }
                            None => {}
                            _ => {}
                        },
                        KeyCode::Right => match self.active_input {
                            Some(InputType::CreatingColumn)
                            | Some(InputType::CreatingTaskTitle)
                            | Some(InputType::CreatingTaskDescription) => {
                                let event = crossterm::event::Event::Key(key);
                                match &mut self.active_input {
                                    Some(InputType::CreatingColumn) => {
                                        if let Some(popup) = &mut self.create_column_popup {
                                            popup.input.handle_event(&event);
                                        }
                                    }
                                    Some(InputType::CreatingTaskTitle) => {
                                        if let Some(popup) = &mut self.creating_task_popup {
                                            popup.title.handle_event(&event);
                                        } else if let Some(popup) = &mut self.edit_task_popup {
                                            popup.title.handle_event(&event);
                                        }
                                    }
                                    Some(InputType::CreatingTaskDescription) => {
                                        let textarea_event: Input = event::Event::Key(key).into();
                                        if let Some(popup) = &mut self.creating_task_popup {
                                            popup.description.input(textarea_event);
                                        } else if let Some(popup) = &mut self.edit_task_popup {
                                            popup.description.input(textarea_event);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            Some(InputType::CreatingTaskPriority) => {
                                if let Some(popup) = &mut self.creating_task_popup {
                                    popup.priority = match popup.priority {
                                        TaskPriority::LOW => TaskPriority::MEDIUM,
                                        TaskPriority::MEDIUM => TaskPriority::HIGH,
                                        TaskPriority::HIGH => TaskPriority::LOW,
                                    };
                                } else if let Some(popup) = &mut self.edit_task_popup {
                                    popup.priority = match popup.priority {
                                        TaskPriority::LOW => TaskPriority::MEDIUM,
                                        TaskPriority::MEDIUM => TaskPriority::HIGH,
                                        TaskPriority::HIGH => TaskPriority::LOW,
                                    };
                                }
                            }
                            None => {}
                        },
                        KeyCode::Left => match self.active_input {
                            Some(InputType::CreatingColumn)
                            | Some(InputType::CreatingTaskTitle)
                            | Some(InputType::CreatingTaskDescription) => {
                                let event = crossterm::event::Event::Key(key);
                                match &mut self.active_input {
                                    Some(InputType::CreatingColumn) => {
                                        if let Some(popup) = &mut self.create_column_popup {
                                            popup.input.handle_event(&event);
                                        }
                                    }
                                    Some(InputType::CreatingTaskTitle) => {
                                        if let Some(popup) = &mut self.creating_task_popup {
                                            popup.title.handle_event(&event);
                                        } else if let Some(popup) = &mut self.edit_task_popup {
                                            popup.title.handle_event(&event);
                                        }
                                    }
                                    Some(InputType::CreatingTaskDescription) => {
                                        let textarea_event: Input = event::Event::Key(key).into();
                                        if let Some(popup) = &mut self.creating_task_popup {
                                            popup.description.input(textarea_event);
                                        } else if let Some(popup) = &mut self.edit_task_popup {
                                            popup.description.input(textarea_event);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            Some(InputType::CreatingTaskPriority) => {
                                if let Some(popup) = &mut self.creating_task_popup {
                                    popup.priority = match popup.priority {
                                        TaskPriority::LOW => TaskPriority::HIGH,
                                        TaskPriority::MEDIUM => TaskPriority::LOW,
                                        TaskPriority::HIGH => TaskPriority::MEDIUM,
                                    };
                                } else if let Some(popup) = &mut self.edit_task_popup {
                                    popup.priority = match popup.priority {
                                        TaskPriority::LOW => TaskPriority::HIGH,
                                        TaskPriority::MEDIUM => TaskPriority::LOW,
                                        TaskPriority::HIGH => TaskPriority::MEDIUM,
                                    };
                                }
                            }
                            None => {}
                        },
                        _ => {
                            let event: Input = crossterm::event::Event::Key(key).into();
                            if matches!(self.active_input, Some(InputType::CreatingTaskDescription))
                            {
                                if let Some(popup) = &mut self.creating_task_popup {
                                    popup.description.input(event);
                                } else if let Some(popup) = &mut self.edit_task_popup {
                                    popup.description.input(event);
                                }
                            }
                        }
                    },
                    InputMode::Visual => match key.code {
                        KeyCode::Esc => {
                            self.visual_selection_active = false;
                            self.visual_selected_tasks_indexes.clear();
                            self.visual_selected_columns_indexes.clear();
                            self.visual_selection_direction = None;
                            self.visual_anchor_task = self.focused_task;
                            self.visual_anchor_column = self.focused_column;
                            self.input_mode = InputMode::Normal;
                        }
                        KeyCode::Left => {
                            let is_selecting_tasks = !self.visual_selected_tasks_indexes.is_empty();
                            if !is_selecting_tasks {
                                if self.visual_selected_columns_indexes.is_empty() {
                                    self.visual_selected_columns_indexes =
                                        vec![self.visual_anchor_column];
                                    self.visual_selection_direction =
                                        Some(VisualSelectionDirection::LEFT);
                                } else if self.visual_selected_columns_indexes.len() == 1
                                    && matches!(
                                        self.visual_selection_direction,
                                        Some(VisualSelectionDirection::RIGHT)
                                    )
                                {
                                    self.visual_selected_columns_indexes.clear();
                                } else {
                                    self.focused_column = self.focused_column.saturating_sub(1);
                                    let start = self.visual_anchor_column.min(self.focused_column);
                                    let end = self.visual_anchor_column.max(self.focused_column);
                                    self.visual_selected_columns_indexes = (start..=end).collect();
                                    self.visual_selected_tasks_indexes.clear();
                                }
                            }
                        }
                        KeyCode::Up => {
                            let is_selecting_columns =
                                !self.visual_selected_columns_indexes.is_empty();
                            if !is_selecting_columns {
                                if self.visual_selected_tasks_indexes.is_empty() {
                                    self.visual_selected_tasks_indexes =
                                        vec![(self.visual_anchor_column, self.visual_anchor_task)];
                                    self.visual_selection_direction =
                                        Some(VisualSelectionDirection::UP);
                                } else if self.visual_selected_tasks_indexes.len() == 1
                                    && matches!(
                                        self.visual_selection_direction,
                                        Some(VisualSelectionDirection::DOWN)
                                    )
                                {
                                    self.visual_selected_tasks_indexes.clear();
                                } else {
                                    self.focused_task = self.focused_task.saturating_sub(1);
                                    let start = self.visual_anchor_task.min(self.focused_task);
                                    let end = self.visual_anchor_task.max(self.focused_task);
                                    self.visual_selected_tasks_indexes =
                                        (start..=end).map(|t| (self.focused_column, t)).collect();
                                    self.visual_selected_columns_indexes.clear();
                                }
                            }
                        }
                        KeyCode::Right => {
                            let is_selecting_tasks = !self.visual_selected_tasks_indexes.is_empty();
                            if !is_selecting_tasks {
                                if self.visual_selected_columns_indexes.is_empty() {
                                    self.visual_selected_columns_indexes =
                                        vec![self.visual_anchor_column];
                                    self.visual_selection_direction =
                                        Some(VisualSelectionDirection::RIGHT);
                                } else if self.visual_selected_columns_indexes.len() == 1
                                    && matches!(
                                        self.visual_selection_direction,
                                        Some(VisualSelectionDirection::LEFT)
                                    )
                                {
                                    self.visual_selected_columns_indexes.clear();
                                } else {
                                    self.focused_column = self
                                        .focused_column
                                        .saturating_add(1)
                                        .min(self.columns.len() - 1);
                                    let start = self.visual_anchor_column.min(self.focused_column);
                                    let end = self.visual_anchor_column.max(self.focused_column);
                                    self.visual_selected_columns_indexes = (start..=end).collect();
                                    self.visual_selected_tasks_indexes.clear();
                                }
                            }
                        }
                        KeyCode::Down => {
                            let is_selecting_columns =
                                !self.visual_selected_columns_indexes.is_empty();
                            if !is_selecting_columns {
                                if self.visual_selected_tasks_indexes.is_empty() {
                                    self.visual_selected_tasks_indexes =
                                        vec![(self.visual_anchor_column, self.visual_anchor_task)];
                                    self.visual_selection_direction =
                                        Some(VisualSelectionDirection::DOWN);
                                } else if self.visual_selected_tasks_indexes.len() == 1
                                    && matches!(
                                        self.visual_selection_direction,
                                        Some(VisualSelectionDirection::UP)
                                    )
                                {
                                    self.visual_selected_tasks_indexes.clear();
                                } else {
                                    self.focused_task = self.focused_task.saturating_add(1).min(
                                        self.columns[self.focused_column]
                                            .tasks
                                            .len()
                                            .saturating_sub(1),
                                    );
                                    let start = self.visual_anchor_task.min(self.focused_task);
                                    let end = self.visual_anchor_task.max(self.focused_task);
                                    self.visual_selected_tasks_indexes =
                                        (start..=end).map(|t| (self.focused_column, t)).collect();
                                    self.visual_selected_columns_indexes.clear();
                                }
                            }
                        }
                        // KeyCode::Tab => {
                        //     self.focused_column = (self.focused_column + 1).min(self.columns.len() - 1);
                        //     self.visual_anchor_task = self.focused_task;
                        //     self.focused_task = 0;
                        // }
                        // KeyCode::BackTab => {
                        //     self.focused_column = self.focused_column.saturating_sub(1);
                        //     self.visual_anchor_task = self.focused_task;
                        //     self.focused_task = 0;
                        // }
                        KeyCode::Char('s') if key.modifiers == KeyModifiers::CONTROL => {
                            let is_selecting_tasks = !self.visual_selected_tasks_indexes.is_empty()
                                && self.visual_selected_columns_indexes.is_empty();
                            if is_selecting_tasks {
                                self.input_mode = InputMode::Normal;
                            }
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
        }
    }

    fn open_popup(&mut self, popup: Popup) {
        match popup {
            Popup::DeleteColumn => self.delete_column_popup = Some(DeleteColumnPopup::new()),
            Popup::EditTask => {
                let focused_task =
                    self.columns[self.focused_column].tasks[self.focused_task].clone();
                self.edit_task_popup = Some(CreatingTaskPopup::from(focused_task));
                self.active_input = Some(InputType::CreatingTaskTitle);
            }
        }
    }

    fn close_popup(&mut self, popup: Popup) {
        match popup {
            Popup::DeleteColumn => self.delete_column_popup = None,
            Popup::EditTask => {
                self.edit_task_popup = None;
                self.active_input = None;
            }
        }
    }

    fn safe_create_task(&mut self) -> bool {
        if let Some(popup) = &self.creating_task_popup {
            if popup.title.value().is_empty() || popup.description.is_empty() {
                // TODO : DISPLAY ERROR WHEN EMPTY ON CREATION
                return true;
            }

            let task: Task = Task::new(
                popup.title.value().to_string(),
                popup.description.lines().join("\n"),
                popup.priority,
                self.columns[self.focused_column].tasks.len(),
            );
            self.columns[self.focused_column].tasks.push(task);
            self.persist_cards();
        }
        self.active_input = None;
        self.input_mode = InputMode::Normal;
        self.creating_task_popup = None;
        false
    }

    fn safe_edit_task(&mut self) {
        if let Some(popup) = &self.edit_task_popup {
            let column = &mut self.columns[self.focused_column];
            let task = &mut column.tasks[self.focused_task];

            task.title = popup.title.value().to_string();
            task.description = popup.description.lines().join("\n");
            task.priority = popup.priority;

            self.active_input = None;
            self.input_mode = InputMode::Normal;
            self.close_popup(Popup::EditTask);
            self.persist_cards();
        }
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

    fn is_not_bulk(&self) -> bool {
        !self.visual_selection_active && !matches!(self.input_mode, InputMode::Visual)
    }

    fn print_mode(&self) -> String {
        match self.input_mode {
            InputMode::Normal => String::from("Normal"),
            InputMode::Editing => String::from("Editing"),
            InputMode::Visual => String::from("Visual"),
        }
    }

    fn render(&mut self, frame: &mut Frame) {
        let layout = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ]);

        let [help, kanban, status] = frame.area().layout(&layout);

        frame.render_widget(
            Paragraph::new(format!(
                "{}  {}  {} repositories  {} annotations",
                self.workspace_config.name,
                self.workspace_root.display(),
                self.workspace_config.repositories.len(),
                self.findings_count
            ))
            .style(Style::default().add_modifier(Modifier::BOLD)),
            help,
        );
        self.render_columns(frame, kanban);
        let (status_text, status_style) = if let Some(error) = &self.last_error {
            (error.clone(), Style::default().fg(Color::Red))
        } else if let Some(warning) = self.warnings.first() {
            (warning.clone(), Style::default().fg(Color::Yellow))
        } else {
            (
                format!("Mode: {}", self.print_mode()),
                Style::default().fg(Color::DarkGray),
            )
        };
        frame.render_widget(Paragraph::new(status_text).style(status_style), status);
    }

    fn render_columns(&mut self, frame: &mut Frame, area: Rect) {
        let content_width = self.compute_total_width();
        self.scroll_x_state = self
            .scroll_x_state
            .content_length(content_width.saturating_sub(self.area_width as usize))
            .position(self.scroll_x);
        self.area_width = area.width;
        let needs_scroll = content_width > self.area_width as usize;

        let columns_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: if needs_scroll {
                area.height.saturating_sub(1)
            } else {
                area.height
            },
        };

        let visible_on_the_left = self.scroll_x as u16;
        let visible_on_the_right = (self.scroll_x as u16) + columns_area.width;

        for i in 0..self.columns.len() {
            let ongoing_column = &self.columns[i];

            let column_left = i as u16 * (ongoing_column.width as u16 + COLUMNS_GAP);
            let column_rigt = column_left + ongoing_column.width as u16;

            if column_rigt <= visible_on_the_left || column_left >= visible_on_the_right {
                continue;
            }

            let screen_x = columns_area
                .x
                .saturating_add(column_left.saturating_sub(visible_on_the_left));
            let hidden_left = visible_on_the_left.saturating_sub(column_left);
            let visible_width = (ongoing_column.width as u16)
                .saturating_sub(hidden_left)
                .min(columns_area.right().saturating_sub(screen_x));

            if visible_width == 0 {
                continue;
            }

            let column = Rect {
                x: screen_x,
                y: columns_area.y,
                width: visible_width,
                height: columns_area.height,
            };

            let ongoing_column = &self.columns[i];
            let is_visual_col = self.visual_selected_columns_indexes.contains(&i);
            let block = Block::bordered()
                .title(ongoing_column.name.clone())
                .border_style(if i == self.focused_column {
                    Style::new().bold().fg(if is_visual_col {
                        Color::Cyan
                    } else {
                        Color::White
                    })
                } else if is_visual_col {
                    Style::new().fg(Color::Cyan)
                } else {
                    Style::default()
                })
                .style(if is_visual_col {
                    Style::new().bg(Color::DarkGray)
                } else {
                    Style::default()
                });

            let inner = block.inner(column);
            frame.render_widget(block, column);

            let task_constraints: Vec<Constraint> = ongoing_column
                .tasks
                .iter()
                .map(|_| Constraint::Length(3))
                .collect();

            if !task_constraints.is_empty() {
                let task_areas = Layout::vertical(task_constraints).split(inner);
                for (j, task) in ongoing_column.tasks.iter().enumerate() {
                    let priority_color = match task.priority {
                        TaskPriority::LOW => Color::Blue,
                        TaskPriority::MEDIUM => Color::Yellow,
                        TaskPriority::HIGH => Color::Red,
                    };

                    let is_visual_task = self.visual_selected_tasks_indexes.contains(&(i, j));
                    let task_block = Block::bordered()
                        .border_style(if self.focused_task == j && i == self.focused_column {
                            Style::new().bold()
                        } else if is_visual_task {
                            Style::new().fg(Color::Cyan)
                        } else {
                            Style::default().fg(priority_color)
                        })
                        .style(if is_visual_task {
                            Style::new().bg(Color::DarkGray)
                        } else {
                            Style::default()
                        });
                    let task_inner = task_block.inner(task_areas[j]);
                    frame.render_widget(task_block, task_areas[j]);
                    frame.render_widget(Paragraph::new(task.title.as_str()), task_inner);
                }
            }
        }

        if needs_scroll {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::HorizontalBottom);
            frame.render_stateful_widget(
                scrollbar,
                area.inner(Margin {
                    horizontal: 1,
                    vertical: 0,
                }),
                &mut self.scroll_x_state,
            );
        }

        self.render_column_creation_popup(frame, area);

        self.render_task_creation_popup(frame, area);

        self.show_task_popup(frame, area);

        self.render_delete_column_popup(frame, area);
    }

    fn render_delete_column_popup(&mut self, frame: &mut Frame, area: Rect) {
        if let Some(popup) = &self.delete_column_popup {
            let block = Block::default()
                .title("Are you sure you want to delete columns?")
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::DarkGray));

            let popup_area = area.centered(Constraint::Percentage(30), Constraint::Length(5));
            let inner_area = block.inner(popup_area);

            frame.render_widget(Clear, popup_area);
            frame.render_widget(block, popup_area);

            let layout = Layout::vertical([
                Constraint::Fill(1),
                Constraint::Length(3),
                Constraint::Fill(1),
            ]);
            let [_, buttons_row, _] = inner_area.layout(&layout);

            let button_layout = Layout::horizontal([
                Constraint::Fill(1),
                Constraint::Length(15),
                Constraint::Length(10),
                Constraint::Length(15),
                Constraint::Fill(1),
            ]);
            let [_, ok_button, _, cancel_button, _] = buttons_row.layout(&button_layout);

            let ok_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .style(Style::default());

            let cancel_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .style(Style::default());

            let ok_text_style = if popup.delete {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };

            let cancel_text_style = if !popup.delete {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            let inner_ok = ok_block.inner(ok_button);
            let inner_cancel = cancel_block.inner(cancel_button);

            frame.render_widget(ok_block, ok_button);
            frame.render_widget(cancel_block, cancel_button);

            frame.render_widget(
                Paragraph::new("[ delete ]").centered().style(ok_text_style),
                inner_ok,
            );
            frame.render_widget(
                Paragraph::new("[ cancel ]")
                    .centered()
                    .style(cancel_text_style),
                inner_cancel,
            );
        }
    }

    fn render_input(&self, frame: &mut Frame, area: Rect) {
        let width = area.width.max(3) - 3;
        let source_input = match &self.active_input {
            Some(InputType::CreatingColumn) => self.create_column_popup.as_ref().map(|p| &p.input),
            Some(InputType::CreatingTaskTitle) => {
                self.creating_task_popup.as_ref().map(|p| &p.title)
            }
            _ => None,
        };

        if let Some(source_input) = source_input {
            let scroll = source_input.visual_scroll(width as usize);
            let input = Paragraph::new(source_input.value())
                .style(Color::Yellow)
                .scroll((0, scroll as u16))
                .block(Block::bordered().title("Input"));
            frame.render_widget(input, area);

            if matches!(self.input_mode, InputMode::Editing) {
                let x = source_input.visual_cursor().max(scroll) - scroll + 1;
                frame.set_cursor_position((area.x + x as u16, area.y + 1));
            }
        }
    }

    fn render_column_creation_popup(&mut self, frame: &mut Frame, area: Rect) {
        if self.create_column_popup.is_some() {
            let popup_block = Block::default()
                .title("Creating a new column")
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::DarkGray));

            let area = area.centered(Constraint::Percentage(30), Constraint::Length(3));

            frame.render_widget(Clear, area);
            frame.render_widget(popup_block, area);
            self.render_input(frame, area);
        }
    }

    fn show_task_popup(&mut self, frame: &mut Frame, area: Rect) {
        if let Some(popup) = &self.displaying_task_popup {
            let popup_block = Block::default()
                .borders(Borders::ALL)
                .title_bottom(" Press d to mark task as done ")
                .title_alignment(HorizontalAlignment::Right)
                .style(Style::default().bg(Color::DarkGray));

            let description_lines = popup.task.description.clone().lines().count();
            let popup_height = 3 + description_lines.max(1) + 3;

            let popup_area = area.centered(
                Constraint::Percentage(30),
                Constraint::Length(popup_height as u16),
            );
            let inner_area = popup_block.inner(popup_area);

            frame.render_widget(Clear, popup_area);

            let layout = Layout::vertical([
                Constraint::Length(4),
                Constraint::Length(popup_height as u16),
            ]);
            let [top_area, description_area] = inner_area.layout(&layout);
            frame.render_widget(popup_block, popup_area);

            let [title_area, priority_area] = top_area.layout(&Layout::horizontal([
                Constraint::Percentage(70),
                Constraint::Percentage(30),
            ]));

            let title_block = Block::default().borders(Borders::NONE);
            let inner_title_area = title_block.inner(title_area);
            frame.render_widget(title_block, title_area);
            frame.render_widget(Paragraph::new(popup.task.title.clone()), inner_title_area);

            let mut priority_block = Block::default().borders(Borders::NONE);
            let inner_priority_area = priority_block.inner(priority_area);
            let priority_style = match popup.task.priority {
                TaskPriority::LOW => Style::default().fg(Color::Blue),
                TaskPriority::MEDIUM => Style::default().fg(Color::Yellow),
                TaskPriority::HIGH => Style::default().fg(Color::Red),
            };

            priority_block = priority_block.border_style(priority_style);
            frame.render_widget(priority_block, priority_area);
            frame.render_widget(
                Paragraph::new(popup.task.priority.value())
                    .alignment(HorizontalAlignment::Right)
                    .style(priority_style),
                inner_priority_area,
            );

            let description_block = Block::default().borders(Borders::NONE);
            let inner_description_area = description_block.inner(description_area);
            frame.render_widget(description_block, description_area);
            frame.render_widget(
                Paragraph::new(popup.task.description.clone()).wrap(Wrap::default()),
                inner_description_area,
            );
        }
    }

    fn render_task_creation_popup(&mut self, frame: &mut Frame, area: Rect) {
        if let Some(t_popup) = &mut self.creating_task_popup {
            let popup_block = Block::default()
                .title("Create a new task")
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::DarkGray));

            let area = area.centered(Constraint::Percentage(60), Constraint::Min(9));
            let inner_area = popup_block.inner(area);

            let layout = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(3),
                Constraint::Length(3),
            ]);
            let [title_area, description_area, priority_area] = inner_area.layout(&layout);

            frame.render_widget(Clear, area);
            frame.render_widget(popup_block, area);

            // Title
            let title_block = Block::default()
                .title("Title")
                .borders(Borders::ALL)
                .border_style(
                    if matches!(self.active_input, Some(InputType::CreatingTaskTitle)) {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default()
                    },
                );
            let title_scroll = t_popup
                .title
                .visual_scroll((title_area.width.max(3) - 3) as usize);
            frame.render_widget(
                Paragraph::new(t_popup.title.value())
                    .scroll((0, title_scroll as u16))
                    .block(title_block),
                title_area,
            );
            if matches!(self.active_input, Some(InputType::CreatingTaskTitle)) {
                let x = t_popup.title.visual_cursor().max(title_scroll) - title_scroll + 1;
                frame.set_cursor_position((title_area.x + x as u16, title_area.y + 1));
            }

            t_popup.description.set_block(
                Block::default()
                    .title("Description")
                    .borders(Borders::ALL)
                    .border_style(
                        if matches!(self.active_input, Some(InputType::CreatingTaskDescription)) {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default()
                        },
                    ),
            );

            if matches!(self.active_input, Some(InputType::CreatingTaskDescription)) {
                t_popup.description.set_cursor_style(
                    Style::default().add_modifier(ratatui::style::Modifier::REVERSED),
                );
            } else {
                t_popup.description.set_cursor_style(Style::default());
            }

            frame.render_widget(&t_popup.description, description_area);

            let priority_block = Block::default()
                .title("Priority")
                .borders(Borders::ALL)
                .border_style(
                    if matches!(self.active_input, Some(InputType::CreatingTaskPriority)) {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default()
                    },
                );
            let inner_priority = priority_block.inner(priority_area);
            frame.render_widget(priority_block, priority_area);

            let priority_layout = Layout::horizontal([
                Constraint::Fill(1),
                Constraint::Fill(1),
                Constraint::Fill(1),
            ]);
            let [low_area, medium_area, high_area] = inner_priority.layout(&priority_layout);

            let low_style = if matches!(t_popup.priority, TaskPriority::LOW) {
                Style::default().bg(Color::Blue)
            } else {
                Style::default()
            };
            let medium_style = if matches!(t_popup.priority, TaskPriority::MEDIUM) {
                Style::default().bg(Color::Yellow)
            } else {
                Style::default()
            };
            let high_style = if matches!(t_popup.priority, TaskPriority::HIGH) {
                Style::default().bg(Color::Red)
            } else {
                Style::default()
            };

            frame.render_widget(
                Paragraph::new("LOW")
                    .style(low_style)
                    .alignment(ratatui::layout::Alignment::Center),
                low_area,
            );
            frame.render_widget(
                Paragraph::new("MEDIUM")
                    .style(medium_style)
                    .alignment(ratatui::layout::Alignment::Center),
                medium_area,
            );
            frame.render_widget(
                Paragraph::new("HIGH")
                    .style(high_style)
                    .alignment(ratatui::layout::Alignment::Center),
                high_area,
            );
        } else if let Some(t_popup) = &mut self.edit_task_popup {
            let popup_block = Block::default()
                .title(format!("Edit task {}", t_popup.title))
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::DarkGray));

            let area = area.centered(Constraint::Percentage(60), Constraint::Min(9));
            let inner_area = popup_block.inner(area);

            let layout = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(3),
                Constraint::Length(3),
            ]);
            let [title_area, description_area, priority_area] = inner_area.layout(&layout);

            frame.render_widget(Clear, area);
            frame.render_widget(popup_block, area);

            // Title
            let title_block = Block::default()
                .title("Title")
                .borders(Borders::ALL)
                .border_style(
                    if matches!(self.active_input, Some(InputType::CreatingTaskTitle)) {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default()
                    },
                );
            let title_scroll = t_popup
                .title
                .visual_scroll((title_area.width.max(3) - 3) as usize);
            frame.render_widget(
                Paragraph::new(t_popup.title.value())
                    .scroll((0, title_scroll as u16))
                    .block(title_block),
                title_area,
            );
            if matches!(self.active_input, Some(InputType::CreatingTaskTitle)) {
                let x = t_popup.title.visual_cursor().max(title_scroll) - title_scroll + 1;
                frame.set_cursor_position((title_area.x + x as u16, title_area.y + 1));
            }

            t_popup.description.set_block(
                Block::default()
                    .title("Description")
                    .borders(Borders::ALL)
                    .border_style(
                        if matches!(self.active_input, Some(InputType::CreatingTaskDescription)) {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default()
                        },
                    ),
            );

            if matches!(self.active_input, Some(InputType::CreatingTaskDescription)) {
                t_popup.description.set_cursor_style(
                    Style::default().add_modifier(ratatui::style::Modifier::REVERSED),
                );
            } else {
                t_popup.description.set_cursor_style(Style::default());
            }

            frame.render_widget(&t_popup.description, description_area);

            let priority_block = Block::default()
                .title("Priority")
                .borders(Borders::ALL)
                .border_style(
                    if matches!(self.active_input, Some(InputType::CreatingTaskPriority)) {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default()
                    },
                );
            let inner_priority = priority_block.inner(priority_area);
            frame.render_widget(priority_block, priority_area);

            let priority_layout = Layout::horizontal([
                Constraint::Fill(1),
                Constraint::Fill(1),
                Constraint::Fill(1),
            ]);
            let [low_area, medium_area, high_area] = inner_priority.layout(&priority_layout);

            let low_style = if matches!(t_popup.priority, TaskPriority::LOW) {
                Style::default().bg(Color::Blue)
            } else {
                Style::default()
            };
            let medium_style = if matches!(t_popup.priority, TaskPriority::MEDIUM) {
                Style::default().bg(Color::Yellow)
            } else {
                Style::default()
            };
            let high_style = if matches!(t_popup.priority, TaskPriority::HIGH) {
                Style::default().bg(Color::Red)
            } else {
                Style::default()
            };

            frame.render_widget(
                Paragraph::new("LOW")
                    .style(low_style)
                    .alignment(ratatui::layout::Alignment::Center),
                low_area,
            );
            frame.render_widget(
                Paragraph::new("MEDIUM")
                    .style(medium_style)
                    .alignment(ratatui::layout::Alignment::Center),
                medium_area,
            );
            frame.render_widget(
                Paragraph::new("HIGH")
                    .style(high_style)
                    .alignment(ratatui::layout::Alignment::Center),
                high_area,
            );
        }
    }
    fn toggle_column_creation_popup(&mut self) {
        match self.create_column_popup {
            Some(_) => {
                self.create_column_popup = None;
                self.active_input = None;
            }
            None => {
                self.create_column_popup = Some(CreateColumnPopup::new());
                self.active_input = Some(InputType::CreatingColumn);
            }
        }
    }

    fn toggle_task_displaying_popup(&mut self, task: Task) {
        match self.displaying_task_popup {
            Some(_) => {
                self.displaying_task_popup = None;
            }
            None => {
                self.displaying_task_popup = Some(DisplayingTaskPopup::new(task));
            }
        }
    }

    fn toggle_task_creation_popup(&mut self) {
        match self.creating_task_popup {
            Some(_) => {
                self.creating_task_popup = None;
                self.active_input = None;
            }
            None => {
                self.creating_task_popup = Some(CreatingTaskPopup::new());
                self.active_input = Some(InputType::CreatingTaskTitle)
            }
        }
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

    fn compute_total_width(&self) -> usize {
        let mut total_width = 0;
        for column in &self.columns {
            total_width += column.width;
        }

        total_width
    }

    fn send_task_to_next(&mut self) {
        if self.focused_column == self.columns.len() - 1
            || self.columns[self.focused_column].tasks.is_empty()
        {
            return;
        }

        self.send_task_to_column(
            self.focused_task,
            self.focused_column,
            self.focused_column + 1,
        );
        self.focused_column = self.focused_column.saturating_add(1);
        self.focused_task = self.columns[self.focused_column].tasks.len() - 1;
        self.persist_ui_state();
    }
    fn send_task_to_prev(&mut self) {
        if self.focused_column == 0 || self.columns[self.focused_column].tasks.is_empty() {
            return;
        }

        self.send_task_to_column(
            self.focused_task,
            self.focused_column,
            self.focused_column - 1,
        );
        self.focused_column = self.focused_column.saturating_sub(1);
        self.focused_task = self.columns[self.focused_column].tasks.len() - 1;
        self.persist_ui_state();
    }

    fn send_task_to_column(
        &mut self,
        task_index: usize,
        source_column_index: usize,
        target_column_index: usize,
    ) {
        let task = self.columns[source_column_index].tasks[task_index].clone();
        self.columns[source_column_index].tasks.remove(task_index);
        self.columns[target_column_index].tasks.push(task);
        self.persist_cards();
    }

    fn left_space(&self, column_index: usize) -> usize {
        let mut space: usize = 0;
        for i in 0..column_index {
            let col = &self.columns[i];
            space = space.saturating_add(col.width);
            space = space.saturating_add(COLUMNS_GAP as usize);
        }

        space
    }

    fn scroll_to_focused_column(&mut self) {
        let column = &self.columns[self.focused_column];
        let column_l = self.left_space(self.focused_column);
        let column_r = column_l + column.width;

        if column_r > self.scroll_x + self.area_width as usize {
            self.scroll_x = column_r.saturating_sub(self.area_width as usize);
        } else if column_l < self.scroll_x {
            self.scroll_x = column_l;
        }
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
        app.send_task_to_next();
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
}
