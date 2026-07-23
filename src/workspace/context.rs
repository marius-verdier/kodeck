use std::path::PathBuf;

use crate::domain::WorkspaceConfig;
use crate::storage::WorkspacePaths;

use super::{PrivateWorkspaceState, PrivateWorkspaceStore, SharedWorkspaceStore};

#[derive(Debug, Clone)]
pub struct WorkspaceContext {
    pub root: PathBuf,
    pub paths: WorkspacePaths,
    pub config: WorkspaceConfig,
    pub private_state: PrivateWorkspaceState,
    pub warnings: Vec<String>,
    pub(crate) shared_store: SharedWorkspaceStore,
    pub(crate) private_store: PrivateWorkspaceStore,
}

impl WorkspaceContext {
    pub fn shared_store(&self) -> &SharedWorkspaceStore {
        &self.shared_store
    }

    pub fn private_store(&self) -> &PrivateWorkspaceStore {
        &self.private_store
    }
}
