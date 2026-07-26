use ratatui_textarea::TextArea;
use std::collections::HashSet;
use std::sync::mpsc::Receiver;
use tui_input::Input;

use crate::domain::{AnnotationId, CardId, ColumnId, Finding, Task, TaskPriority};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AnnotationFilter {
    Unassigned,
    All,
    Missing,
    Ignored,
}

impl AnnotationFilter {
    pub fn next(self) -> Self {
        match self {
            Self::Unassigned => Self::All,
            Self::All => Self::Missing,
            Self::Missing => Self::Ignored,
            Self::Ignored => Self::Unassigned,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Unassigned => "unassigned",
            Self::All => "all",
            Self::Missing => "missing",
            Self::Ignored => "ignored",
        }
    }
}

#[derive(Debug)]
pub(super) struct AnnotationReviewState {
    pub focused: usize,
    pub selected: HashSet<AnnotationId>,
    pub filter: AnnotationFilter,
}

impl Default for AnnotationReviewState {
    fn default() -> Self {
        Self {
            focused: 0,
            selected: HashSet::new(),
            filter: AnnotationFilter::Unassigned,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AnnotationSyncTrigger {
    Open,
    Manual,
}

pub(super) struct AnnotationSyncState {
    pub receiver: Receiver<Vec<Finding>>,
    pub trigger: AnnotationSyncTrigger,
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
    CreatingTaskColumn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TaskFormMode {
    Create,
    Edit(CardId),
    CreateFromAnnotations(Vec<AnnotationId>),
}

pub(super) struct TaskFormState<'a> {
    pub mode: TaskFormMode,
    pub title: Input,
    pub description: TextArea<'a>,
    pub priority: TaskPriority,
    pub target_column: Option<usize>,
    pub error: Option<String>,
}

impl<'a> TaskFormState<'a> {
    pub fn create() -> Self {
        Self {
            mode: TaskFormMode::Create,
            title: Input::default(),
            description: TextArea::default(),
            priority: TaskPriority::LOW,
            target_column: None,
            error: None,
        }
    }

    pub fn edit(task: &Task) -> Self {
        Self {
            mode: TaskFormMode::Edit(task.id),
            title: Input::from(task.title.clone()),
            description: TextArea::from(task.description.split('\n')),
            priority: task.priority,
            target_column: None,
            error: None,
        }
    }

    pub fn from_annotations(
        annotation_ids: Vec<AnnotationId>,
        title: String,
        description: String,
        priority: TaskPriority,
        target_column: usize,
    ) -> Self {
        Self {
            mode: TaskFormMode::CreateFromAnnotations(annotation_ids),
            title: Input::from(title),
            description: TextArea::from(description.lines()),
            priority,
            target_column: Some(target_column),
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
