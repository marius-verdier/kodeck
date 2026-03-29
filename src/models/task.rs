#[derive(Clone, Copy)]
pub enum TaskPriority {
    HIGH,
    MEDIUM,
    LOW
}

impl TaskPriority {
    pub fn value(&self) -> String {
        match self {
            TaskPriority::HIGH => String::from("high"),
            TaskPriority::MEDIUM => String::from("medium"),
            TaskPriority::LOW => String::from("low")
        }
    }

    pub fn from_value(value: &str) -> Option<TaskPriority> {
        match value {
            "high" => Some(TaskPriority::HIGH),
            "medium" => Some(TaskPriority::MEDIUM),
            "low" => Some(TaskPriority::LOW),
            _ => None
        }
    }
}

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
            order
        }
    }
}

pub struct CreatingTaskPopup {
    pub title: String,
    pub description: String,
    pub priority: TaskPriority,
}

impl CreatingTaskPopup {
    pub fn new() -> CreatingTaskPopup {
        CreatingTaskPopup {
            title: String::new(),
            description: String::new(),
            priority: TaskPriority::LOW,
        }
    }
}