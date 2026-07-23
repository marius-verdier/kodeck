use super::{ColumnId, Task};

pub struct Column {
    pub id: ColumnId,
    pub name: String,
    pub width: usize,
    pub tasks: Vec<Task>,
}

impl Column {
    pub fn new(id: ColumnId, name: String, width: usize) -> Column {
        Column {
            id,
            name,
            width,
            tasks: Vec::new(),
        }
    }
}
