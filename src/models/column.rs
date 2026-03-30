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

pub struct CreatingColumnPopup {
    pub input: Input,
}

impl CreatingColumnPopup {
    pub fn new() -> CreatingColumnPopup {
        CreatingColumnPopup { input: Input::default() }
    }
}