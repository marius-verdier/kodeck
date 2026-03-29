use crate::models::task::Task;

pub struct Column {
    pub name: String,
    pub width: usize,
    pub tasks: Vec<Task>,
}

impl Column {
    pub fn new(name: String, width: usize) -> Column {
        Column { name, width, tasks: Vec::new() }
    }
}

pub struct CreatingColumnPopup {
    pub input: String,
}

impl CreatingColumnPopup {
    pub fn new() -> CreatingColumnPopup {
        CreatingColumnPopup { input: String::new() }
    }
}