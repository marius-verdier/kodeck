use tui_input::Input;
use crate::models::task::Task;


pub struct Column {
    pub name: String,
    pub width: usize,
    pub tasks: Vec<Task>,
}

impl Column {
    pub fn new(name: String, width: usize) -> Column {
        Column {
            name,
            width,
            tasks: Vec::new()
        }
    }
}

pub struct CreateColumnPopup {
    pub input: Input,
}

impl CreateColumnPopup {
    pub fn new() -> Self {
        Self { input: Input::default() }
    }
}

pub struct DeleteColumnPopup {
    pub delete: bool,
}

impl DeleteColumnPopup {
    pub fn new() -> Self {
        Self { delete: false }
    }
}