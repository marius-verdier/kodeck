use crate::domain::{
    CacheFile, CardsFile, SyncQueueFile, UiStateFile, WorkspaceConfig, WorkspaceId,
};
use crate::storage::{AppPaths, AtomicJsonStore, WorkspacePaths};

use super::WorkspaceError;

#[derive(Debug, Clone)]
pub struct SharedWorkspaceStore {
    store: AtomicJsonStore,
}

impl SharedWorkspaceStore {
    pub fn from_root(root: impl AsRef<std::path::Path>) -> Self {
        Self {
            store: AtomicJsonStore::shared(AppPaths::shared_workspace_file(root)),
        }
    }

    pub fn new(paths: &WorkspacePaths) -> Self {
        Self {
            store: AtomicJsonStore::shared(paths.shared_config_file()),
        }
    }

    pub fn exists(&self) -> bool {
        self.store.exists()
    }

    pub fn load(&self) -> Result<WorkspaceConfig, WorkspaceError> {
        let workspace: WorkspaceConfig = self.store.read()?;
        workspace.validate()?;
        Ok(workspace)
    }

    pub fn save(&self, workspace: &WorkspaceConfig) -> Result<(), WorkspaceError> {
        workspace.validate()?;
        self.store.write(workspace)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct PrivateWorkspaceStore {
    workspace_id: WorkspaceId,
    cards: AtomicJsonStore,
    state: AtomicJsonStore,
    cache: AtomicJsonStore,
    sync_queue: AtomicJsonStore,
}

impl PrivateWorkspaceStore {
    pub fn new(paths: &WorkspacePaths, workspace_id: WorkspaceId) -> Self {
        Self {
            workspace_id,
            cards: AtomicJsonStore::private(paths.cards_file()),
            state: AtomicJsonStore::private(paths.state_file()),
            cache: AtomicJsonStore::private(paths.cache_file()),
            sync_queue: AtomicJsonStore::private(paths.sync_queue_file()),
        }
    }

    pub fn ensure_and_load(&self) -> Result<PrivateWorkspaceState, WorkspaceError> {
        let cards = self.load_or_create_cards()?;
        let state = self.load_or_create_ui_state()?;
        let cache = self.load_or_create_cache()?;
        let sync_queue = self.load_or_create_sync_queue()?;
        Ok(PrivateWorkspaceState {
            cards,
            state,
            cache,
            sync_queue,
        })
    }

    pub fn load_cards(&self) -> Result<CardsFile, WorkspaceError> {
        let value: CardsFile = self.cards.read()?;
        value.validate_for(self.workspace_id)?;
        Ok(value)
    }

    pub fn save_cards(&self, value: &CardsFile) -> Result<(), WorkspaceError> {
        value.validate_for(self.workspace_id)?;
        self.cards.write(value)?;
        Ok(())
    }

    pub fn load_ui_state(&self) -> Result<UiStateFile, WorkspaceError> {
        let value: UiStateFile = self.state.read()?;
        value.validate_for(self.workspace_id)?;
        Ok(value)
    }

    pub fn save_ui_state(&self, value: &UiStateFile) -> Result<(), WorkspaceError> {
        value.validate_for(self.workspace_id)?;
        self.state.write(value)?;
        Ok(())
    }

    pub fn load_cache(&self) -> Result<CacheFile, WorkspaceError> {
        let value: CacheFile = self.cache.read()?;
        value.validate_for(self.workspace_id)?;
        Ok(value)
    }

    pub fn save_cache(&self, value: &CacheFile) -> Result<(), WorkspaceError> {
        value.validate_for(self.workspace_id)?;
        self.cache.write(value)?;
        Ok(())
    }

    pub fn load_sync_queue(&self) -> Result<SyncQueueFile, WorkspaceError> {
        let value: SyncQueueFile = self.sync_queue.read()?;
        value.validate_for(self.workspace_id)?;
        Ok(value)
    }

    pub fn save_sync_queue(&self, value: &SyncQueueFile) -> Result<(), WorkspaceError> {
        value.validate_for(self.workspace_id)?;
        self.sync_queue.write(value)?;
        Ok(())
    }

    fn load_or_create_cards(&self) -> Result<CardsFile, WorkspaceError> {
        if self.cards.exists() {
            self.load_cards()
        } else {
            let value = CardsFile::empty(self.workspace_id);
            self.save_cards(&value)?;
            Ok(value)
        }
    }

    fn load_or_create_ui_state(&self) -> Result<UiStateFile, WorkspaceError> {
        if self.state.exists() {
            self.load_ui_state()
        } else {
            let value = UiStateFile::empty(self.workspace_id);
            self.save_ui_state(&value)?;
            Ok(value)
        }
    }

    fn load_or_create_cache(&self) -> Result<CacheFile, WorkspaceError> {
        if self.cache.exists() {
            self.load_cache()
        } else {
            let value = CacheFile::empty(self.workspace_id);
            self.save_cache(&value)?;
            Ok(value)
        }
    }

    fn load_or_create_sync_queue(&self) -> Result<SyncQueueFile, WorkspaceError> {
        if self.sync_queue.exists() {
            self.load_sync_queue()
        } else {
            let value = SyncQueueFile::empty(self.workspace_id);
            self.save_sync_queue(&value)?;
            Ok(value)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateWorkspaceState {
    pub cards: CardsFile,
    pub state: UiStateFile,
    pub cache: CacheFile,
    pub sync_queue: SyncQueueFile,
}
