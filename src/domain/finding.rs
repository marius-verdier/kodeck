#[derive(Debug, Clone)]
pub struct Finding {
    pub file: String,
    pub line: usize,
    pub tag: String,
    pub message: String,
}
