//! Filesystem and serialization adapters.
//!
//! Concrete persistence is kept separate from the domain and TUI.

mod atomic_json;
mod paths;

pub use atomic_json::{AtomicJsonStore, JsonStoreError, StoragePermissions};
pub use paths::{AppPaths, AppPathsError, WorkspacePaths};
