use super::RepositoryId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub repository_id: RepositoryId,
    pub path: String,
    pub line: usize,
    pub tag: String,
    pub message: String,
}
