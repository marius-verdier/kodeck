use std::error::Error;
use std::fmt;
use std::io;
use std::path::PathBuf;

use crate::domain::{PrivateStateError, WorkspaceId, WorkspaceValidationErrors};
use crate::storage::{AppPathsError, JsonStoreError};

#[derive(Debug)]
pub enum WorkspaceError {
    Paths(AppPathsError),
    Storage(JsonStoreError),
    Validation(WorkspaceValidationErrors),
    PrivateState(PrivateStateError),
    InvalidGlobalConfig(String),
    InvalidRoot(PathBuf),
    IdentityConflict {
        id: WorkspaceId,
        registered_path: PathBuf,
        discovered_path: PathBuf,
    },
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
}

impl WorkspaceError {
    pub(crate) fn io(operation: &'static str, path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            operation,
            path: path.into(),
            source,
        }
    }
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Paths(error) => error.fmt(formatter),
            Self::Storage(error) => error.fmt(formatter),
            Self::Validation(error) => error.fmt(formatter),
            Self::PrivateState(error) => error.fmt(formatter),
            Self::InvalidGlobalConfig(message) => {
                write!(formatter, "invalid global configuration: {message}")
            }
            Self::InvalidRoot(path) => {
                write!(
                    formatter,
                    "workspace root '{}' is not a directory",
                    path.display()
                )
            }
            Self::IdentityConflict {
                id,
                registered_path,
                discovered_path,
            } => write!(
                formatter,
                "workspace id '{id}' is already active at '{}', not '{}'",
                registered_path.display(),
                discovered_path.display()
            ),
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "failed to {operation} '{}': {source}",
                path.display()
            ),
        }
    }
}

impl Error for WorkspaceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Paths(error) => Some(error),
            Self::Storage(error) => Some(error),
            Self::Validation(error) => Some(error),
            Self::PrivateState(error) => Some(error),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<AppPathsError> for WorkspaceError {
    fn from(value: AppPathsError) -> Self {
        Self::Paths(value)
    }
}

impl From<JsonStoreError> for WorkspaceError {
    fn from(value: JsonStoreError) -> Self {
        Self::Storage(value)
    }
}

impl From<WorkspaceValidationErrors> for WorkspaceError {
    fn from(value: WorkspaceValidationErrors) -> Self {
        Self::Validation(value)
    }
}

impl From<PrivateStateError> for WorkspaceError {
    fn from(value: PrivateStateError) -> Self {
        Self::PrivateState(value)
    }
}
