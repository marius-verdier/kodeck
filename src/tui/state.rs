use ratatui_textarea::TextArea;
use tui_input::Input;

use crate::domain::{CardId, ColumnId, Task, TaskPriority};

pub(super) struct ArchivedTask {
    pub column_id: ColumnId,
    pub task: Task,
}

#[derive(Debug, Clone)]
pub(super) enum Confirmation {
    Archive { card_ids: Vec<CardId> },
    DeleteColumn { column_index: usize },
}

#[derive(Debug, Clone)]
pub(super) struct ConfirmationState {
    pub action: Confirmation,
    pub confirm_selected: bool,
}

#[derive(Debug, Clone)]
pub(super) struct MovePickerState {
    pub selected_column: usize,
}

#[derive(Debug, Clone)]
pub(super) struct GotoTarget {
    pub label: String,
    pub card_id: CardId,
}

#[derive(Debug, Clone)]
pub(super) struct GotoLabelsState {
    pub targets: Vec<GotoTarget>,
    pub input: String,
}

pub(super) struct ColumnFormState {
    pub input: Input,
    pub error: Option<String>,
}

impl ColumnFormState {
    pub fn new() -> Self {
        Self {
            input: Input::default(),
            error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InputMode {
    Normal,
    Editing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub(super) enum InputType {
    CreatingColumn,
    CreatingTaskTitle,
    CreatingTaskDescription,
    CreatingTaskPriority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TaskFormMode {
    Create,
    Edit(CardId),
}

pub(super) struct TaskFormState<'a> {
    pub mode: TaskFormMode,
    pub title: Input,
    pub description: TextArea<'a>,
    pub priority: TaskPriority,
    pub error: Option<String>,
}

impl<'a> TaskFormState<'a> {
    pub fn create() -> Self {
        Self {
            mode: TaskFormMode::Create,
            title: Input::default(),
            description: TextArea::default(),
            priority: TaskPriority::LOW,
            error: None,
        }
    }

    pub fn edit(task: &Task) -> Self {
        Self {
            mode: TaskFormMode::Edit(task.id),
            title: Input::from(task.title.clone()),
            description: TextArea::from(task.description.split('\n')),
            priority: task.priority,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DetailState {
    pub card_id: CardId,
    pub scroll: u16,
}

impl DetailState {
    pub const fn new(card_id: CardId) -> Self {
        Self { card_id, scroll: 0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StatusLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StatusMessage {
    pub level: StatusLevel,
    pub text: String,
}

impl StatusMessage {
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            level: StatusLevel::Info,
            text: text.into(),
        }
    }

    pub fn warn(text: impl Into<String>) -> Self {
        Self {
            level: StatusLevel::Warn,
            text: text.into(),
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            level: StatusLevel::Error,
            text: text.into(),
        }
    }
}
