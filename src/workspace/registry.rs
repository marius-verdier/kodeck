use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::{GLOBAL_CONFIG_SCHEMA_VERSION, GlobalConfig, KnownWorkspace, WorkspaceId};
use crate::storage::{AppPaths, AtomicJsonStore};

use super::WorkspaceError;

#[derive(Debug, Clone)]
pub struct WorkspaceRegistry {
    paths: AppPaths,
    store: AtomicJsonStore,
}

impl WorkspaceRegistry {
    pub fn new(paths: AppPaths) -> Self {
        let store = AtomicJsonStore::private(paths.global_config_file());
        Self { paths, store }
    }

    pub fn load(&self) -> Result<GlobalConfig, WorkspaceError> {
        if !self.store.exists() {
            return Ok(GlobalConfig::default());
        }
        let config: GlobalConfig = self.store.read()?;
        validate_global_config(&config)?;
        Ok(config)
    }

    pub fn save(&self, config: &GlobalConfig) -> Result<(), WorkspaceError> {
        validate_global_config(config)?;
        self.paths.ensure_private_directories().map_err(|source| {
            WorkspaceError::io(
                "create application directories",
                self.paths.data_directory(),
                source,
            )
        })?;
        self.store.write(config)?;
        Ok(())
    }

    pub fn register(
        &self,
        id: WorkspaceId,
        root: impl AsRef<Path>,
    ) -> Result<GlobalConfig, WorkspaceError> {
        let root = absolute_existing_directory(root.as_ref())?;
        let mut config = self.load()?;

        if let Some(conflict) = config
            .known_workspaces
            .iter()
            .find(|workspace| workspace.path == root && workspace.id != id)
        {
            return Err(WorkspaceError::InvalidGlobalConfig(format!(
                "path '{}' is already registered for workspace '{}'",
                root.display(),
                conflict.id
            )));
        }

        if let Some(existing) = config
            .known_workspaces
            .iter_mut()
            .find(|workspace| workspace.id == id)
        {
            existing.path = root;
        } else {
            config
                .known_workspaces
                .push(KnownWorkspace { id, path: root });
        }
        config.last_workspace_id = Some(id);
        self.save(&config)?;
        Ok(config)
    }

    pub fn set_last_workspace(&self, id: WorkspaceId) -> Result<GlobalConfig, WorkspaceError> {
        let mut config = self.load()?;
        if !config
            .known_workspaces
            .iter()
            .any(|workspace| workspace.id == id)
        {
            return Err(WorkspaceError::InvalidGlobalConfig(format!(
                "workspace '{id}' is not registered"
            )));
        }
        config.last_workspace_id = Some(id);
        self.save(&config)?;
        Ok(config)
    }
}

fn validate_global_config(config: &GlobalConfig) -> Result<(), WorkspaceError> {
    if config.schema_version != GLOBAL_CONFIG_SCHEMA_VERSION {
        return Err(WorkspaceError::InvalidGlobalConfig(format!(
            "unsupported schema version {}; expected {}",
            config.schema_version, GLOBAL_CONFIG_SCHEMA_VERSION
        )));
    }

    let mut ids = HashSet::new();
    let mut paths = HashSet::new();
    for workspace in &config.known_workspaces {
        if !workspace.path.is_absolute() {
            return Err(WorkspaceError::InvalidGlobalConfig(format!(
                "workspace path '{}' must be absolute",
                workspace.path.display()
            )));
        }
        if !ids.insert(workspace.id) {
            return Err(WorkspaceError::InvalidGlobalConfig(format!(
                "duplicate workspace id '{}'",
                workspace.id
            )));
        }
        if !paths.insert(&workspace.path) {
            return Err(WorkspaceError::InvalidGlobalConfig(format!(
                "duplicate workspace path '{}'",
                workspace.path.display()
            )));
        }
    }
    if let Some(last_workspace_id) = config.last_workspace_id
        && !ids.contains(&last_workspace_id)
    {
        return Err(WorkspaceError::InvalidGlobalConfig(format!(
            "last workspace id '{last_workspace_id}' is not registered"
        )));
    }
    Ok(())
}

fn absolute_existing_directory(path: &Path) -> Result<PathBuf, WorkspaceError> {
    if !path.is_dir() {
        return Err(WorkspaceError::InvalidRoot(path.to_owned()));
    }
    fs::canonicalize(path)
        .map_err(|source| WorkspaceError::io("canonicalize workspace root", path, source))
}
