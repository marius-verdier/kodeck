use ratatui_textarea::TextArea;
use tui_input::Input;

#[derive(Clone, Copy)]
pub enum TaskPriority {
    HIGH,
    MEDIUM,
    LOW,
}

impl TaskPriority {
    pub fn value(&self) -> String {
        match self {
            TaskPriority::HIGH => String::from("HIGH"),
            TaskPriority::MEDIUM => String::from("MEDIUM"),
            TaskPriority::LOW => String::from("LOW")
        }
    }
    
    pub fn from_value(value: &str) -> Option<TaskPriority> {
        match value {
            "HIGH" => Some(TaskPriority::HIGH),
            "MEDIUM" => Some(TaskPriority::MEDIUM),
            "LOW" => Some(TaskPriority::LOW),
            _ => None
        }
    }
}

#[derive(Clone)]
pub struct Task {
    pub title: String,
    pub description: String,
    pub priority: TaskPriority,
    pub order: usize,
}

impl Task {
    pub fn new(title: String, description: String, priority: TaskPriority, order: usize) -> Task {
        Task {
            title,
            description,
            priority,
            order,
        }
    }
}

pub struct CreatingTaskPopup<'a> {
    pub title: Input,
    pub description: TextArea<'a>,
    pub priority: TaskPriority,
}

impl<'a> CreatingTaskPopup<'a> {
    pub fn new() -> CreatingTaskPopup<'a> {
        CreatingTaskPopup {
            title: Input::default(),
            description: TextArea::default(),
            priority: TaskPriority::LOW,
        }
    }
}

pub struct DisplayingTaskPopup {
    pub task: Task,
}

impl DisplayingTaskPopup {
    pub fn new(task: Task) -> DisplayingTaskPopup {
        DisplayingTaskPopup {
            task,
        }
    }
}