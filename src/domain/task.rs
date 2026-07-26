use serde::{Deserialize, Serialize};

use super::{CardId, LocalCard};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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
            TaskPriority::LOW => String::from("LOW"),
        }
    }

    pub fn from_value(value: &str) -> Option<TaskPriority> {
        match value {
            "HIGH" => Some(TaskPriority::HIGH),
            "MEDIUM" => Some(TaskPriority::MEDIUM),
            "LOW" => Some(TaskPriority::LOW),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    pub id: CardId,
    pub title: String,
    pub description: String,
    pub priority: TaskPriority,
    pub order: usize,
    pub done: bool,
    pub archived_by_sync: bool,
}

impl Task {
    pub fn new(title: String, description: String, priority: TaskPriority, order: usize) -> Task {
        Task {
            id: CardId::generate(),
            title,
            description,
            priority,
            order,
            done: false,
            archived_by_sync: false,
        }
    }

    pub fn from_local_card(card: &LocalCard, order: usize) -> Self {
        Self {
            id: card.id,
            title: card.title.clone(),
            description: card.description.clone(),
            priority: card.priority,
            order,
            done: false,
            archived_by_sync: card.archived_by_sync,
        }
    }
}
