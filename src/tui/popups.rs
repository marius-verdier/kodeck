use ratatui_textarea::TextArea;
use tui_input::Input;

use crate::domain::{Task, TaskPriority};

pub(super) struct CreateColumnPopup {
    pub input: Input,
}

impl CreateColumnPopup {
    pub fn new() -> Self {
        Self {
            input: Input::default(),
        }
    }
}

pub(super) struct DeleteColumnPopup {
    pub delete: bool,
}

impl DeleteColumnPopup {
    pub fn new() -> Self {
        Self { delete: false }
    }
}

pub(super) struct CreatingTaskPopup<'a> {
    pub title: Input,
    pub description: TextArea<'a>,
    pub priority: TaskPriority,
}

impl<'a> CreatingTaskPopup<'a> {
    pub fn new() -> Self {
        Self {
            title: Input::default(),
            description: TextArea::default(),
            priority: TaskPriority::LOW,
        }
    }

    pub fn from(task: Task) -> Self {
        Self {
            title: Input::from(task.title),
            description: TextArea::from(task.description.split('\n')),
            priority: task.priority,
        }
    }
}

pub(super) struct DisplayingTaskPopup {
    pub task: Task,
}

impl DisplayingTaskPopup {
    pub fn new(task: Task) -> Self {
        Self { task }
    }
}
