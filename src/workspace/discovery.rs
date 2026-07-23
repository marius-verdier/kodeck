use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::{KnownWorkspace, WorkspaceId};
use crate::storage::AppPaths;

use super::{
    PrivateWorkspaceStore, RepositoryAvailability, SharedWorkspaceStore, WorkspaceContext,
    WorkspaceError, WorkspaceRegistry, inspect_repositories, suggested_workspace_root,
};

#[derive(Debug, Clone)]
pub enum DiscoveryOutcome {
    Found(Box<WorkspaceContext>),
    SelectionRequired(Vec<KnownWorkspace>),
    NotFound { suggested_root: PathBuf },
}

#[derive(Debug, Clone)]
pub struct WorkspaceManager {
    paths: AppPaths,
    registry: WorkspaceRegistry,
}

impl WorkspaceManager {
    pub fn new(paths: AppPaths) -> Self {
        let registry = WorkspaceRegistry::new(paths.clone());
        Self { paths, registry }
    }

    pub fn discover(&self, start: &Path) -> Result<DiscoveryOutcome, WorkspaceError> {
        let start = canonical_directory(start)?;
        for ancestor in start.ancestors() {
            if AppPaths::shared_workspace_file(ancestor).is_file() {
                return self
                    .open_root(ancestor)
                    .map(Box::new)
                    .map(DiscoveryOutcome::Found);
            }
        }

        let global_config = self.registry.load()?;
        let available: Vec<_> = global_config
            .known_workspaces
            .into_iter()
            .filter(|workspace| AppPaths::shared_workspace_file(&workspace.path).is_file())
            .collect();

        if let Some(last_id) = global_config.last_workspace_id
            && let Some(last) = available.iter().find(|workspace| workspace.id == last_id)
        {
            return self
                .open_root(&last.path)
                .map(Box::new)
                .map(DiscoveryOutcome::Found);
        }

        match available.as_slice() {
            [] => Ok(DiscoveryOutcome::NotFound {
                suggested_root: suggested_workspace_root(&start)?,
            }),
            [workspace] => self
                .open_root(&workspace.path)
                .map(Box::new)
                .map(DiscoveryOutcome::Found),
            _ => Ok(DiscoveryOutcome::SelectionRequired(available)),
        }
    }

    pub fn open_root(&self, root: &Path) -> Result<WorkspaceContext, WorkspaceError> {
        let root = canonical_directory(root)?;
        let shared_store = SharedWorkspaceStore::from_root(&root);
        let config = shared_store.load()?;
        self.reject_live_identity_conflict(config.id, &root)?;

        let paths = self.paths.workspace(&root, config.id);
        self.paths.ensure_private_directories().map_err(|source| {
            WorkspaceError::io(
                "create application directories",
                self.paths.data_directory(),
                source,
            )
        })?;
        paths.ensure_private_directory().map_err(|source| {
            WorkspaceError::io(
                "create private workspace directory",
                paths.private_directory(),
                source,
            )
        })?;
        let private_store = PrivateWorkspaceStore::new(&paths, config.id);
        let private_state = private_store.ensure_and_load()?;
        self.registry.register(config.id, &root)?;

        let mut warnings = Vec::new();
        for repository in inspect_repositories(&root, &config) {
            if let RepositoryAvailability::Missing { id, path } = repository {
                warnings.push(format!(
                    "repository '{id}' is unavailable at '{}'",
                    path.display()
                ));
            }
        }

        Ok(WorkspaceContext {
            root,
            paths,
            config,
            private_state,
            warnings,
            shared_store,
            private_store,
        })
    }

    pub fn registry(&self) -> &WorkspaceRegistry {
        &self.registry
    }

    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }

    fn reject_live_identity_conflict(
        &self,
        id: WorkspaceId,
        discovered_root: &Path,
    ) -> Result<(), WorkspaceError> {
        let registry = self.registry.load()?;
        let Some(registered) = registry
            .known_workspaces
            .iter()
            .find(|workspace| workspace.id == id && workspace.path != discovered_root)
        else {
            return Ok(());
        };
        if !AppPaths::shared_workspace_file(&registered.path).is_file() {
            return Ok(());
        }

        let registered_config = SharedWorkspaceStore::from_root(&registered.path).load()?;
        if registered_config.id == id {
            return Err(WorkspaceError::IdentityConflict {
                id,
                registered_path: registered.path.clone(),
                discovered_path: discovered_root.to_owned(),
            });
        }
        Ok(())
    }
}

fn canonical_directory(path: &Path) -> Result<PathBuf, WorkspaceError> {
    let directory = if path.is_file() {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    if !directory.is_dir() {
        return Err(WorkspaceError::InvalidRoot(directory.to_owned()));
    }
    fs::canonicalize(directory)
        .map_err(|source| WorkspaceError::io("canonicalize directory", directory, source))
}
