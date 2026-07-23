mod context;
mod discovery;
mod error;
mod initialization;
mod registry;
mod repositories;
mod stores;

pub use context::WorkspaceContext;
pub use discovery::{DiscoveryOutcome, WorkspaceManager};
pub use error::WorkspaceError;
pub use initialization::WorkspaceInitializer;
pub use registry::WorkspaceRegistry;
pub use repositories::{RepositoryAvailability, inspect_repositories, suggested_workspace_root};
pub use stores::{PrivateWorkspaceState, PrivateWorkspaceStore, SharedWorkspaceStore};
