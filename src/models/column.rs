pub struct Column {
    pub name: String,
    pub width: usize,
}

impl Column {
    pub fn new(name: String, width: usize) -> Column {
        Column { name, width }
    }

    pub fn factory(quantity: usize) -> Vec<Self> {
        let mut result: Vec<Self> = Vec::new();
        for i in 0..quantity {
            result.push(Self::new(format!("Column{}", i), 35));
        }

        result
    }
}