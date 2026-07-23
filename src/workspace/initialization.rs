use std::fs;
use std::path::Path;

use crate::domain::{WorkspaceConfig, WorkspaceId};
use crate::storage::{AppPaths, WorkspacePaths};

use super::{
    SharedWorkspaceStore, WorkspaceContext, WorkspaceError, WorkspaceManager,
    repositories::detect_repositories,
};

#[derive(Debug, Clone)]
pub struct WorkspaceInitializer {
    manager: WorkspaceManager,
}

impl WorkspaceInitializer {
    pub fn new(paths: AppPaths) -> Self {
        Self {
            manager: WorkspaceManager::new(paths),
        }
    }

    pub fn initialize(
        &self,
        root: &Path,
        name: impl Into<String>,
    ) -> Result<WorkspaceContext, WorkspaceError> {
        if !root.is_dir() {
            return Err(WorkspaceError::InvalidRoot(root.to_owned()));
        }
        let root = fs::canonicalize(root)
            .map_err(|source| WorkspaceError::io("canonicalize workspace root", root, source))?;

        let existing_store = SharedWorkspaceStore::from_root(&root);
        if existing_store.exists() {
            return self.manager.open_root(&root);
        }

        let mut workspace = WorkspaceConfig::new(WorkspaceId::generate(), name);
        workspace.repositories = detect_repositories(&root)?;
        workspace.validate()?;

        let paths: WorkspacePaths = self.manager.paths().workspace(&root, workspace.id);
        let shared_store = SharedWorkspaceStore::new(&paths);
        shared_store.save(&workspace)?;

        self.manager.open_root(&root)
    }

    pub fn manager(&self) -> &WorkspaceManager {
        &self.manager
    }
}
