use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{ColumnId, ProviderId, TaskPriority, WorkspaceId};

pub const PRIVATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CardId(Uuid);

impl CardId {
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl fmt::Display for CardId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalCard {
    pub id: CardId,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub priority: TaskPriority,
    pub column_id: ColumnId,
    #[serde(default)]
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardsFile {
    pub schema_version: u32,
    pub workspace_id: WorkspaceId,
    #[serde(default)]
    pub cards: Vec<LocalCard>,
    #[serde(default)]
    pub ordering: BTreeMap<ColumnId, Vec<CardId>>,
}

impl CardsFile {
    pub fn empty(workspace_id: WorkspaceId) -> Self {
        Self {
            schema_version: PRIVATE_SCHEMA_VERSION,
            workspace_id,
            cards: Vec::new(),
            ordering: BTreeMap::new(),
        }
    }

    pub fn validate_for(&self, workspace_id: WorkspaceId) -> Result<(), PrivateStateError> {
        validate_header(self.schema_version, self.workspace_id, workspace_id)?;

        let mut card_ids = HashSet::new();
        let mut cards_by_id = HashMap::new();
        for card in &self.cards {
            if card.id.as_uuid().is_nil() {
                return Err(PrivateStateError::Invalid(
                    "card UUID must not be nil".to_owned(),
                ));
            }
            if !card_ids.insert(card.id) {
                return Err(PrivateStateError::Invalid(format!(
                    "duplicate card id '{}'",
                    card.id
                )));
            }
            cards_by_id.insert(card.id, card);
            if card.title.trim().is_empty() {
                return Err(PrivateStateError::Invalid(format!(
                    "card '{}' has an empty title",
                    card.id
                )));
            }
        }

        let mut ordered_ids = HashSet::new();
        for (column_id, ids) in &self.ordering {
            for id in ids {
                let Some(card) = cards_by_id.get(id) else {
                    return Err(PrivateStateError::Invalid(format!(
                        "ordering references unknown card '{id}'"
                    )));
                };
                if !ordered_ids.insert(*id) {
                    return Err(PrivateStateError::Invalid(format!(
                        "card '{id}' appears more than once in ordering"
                    )));
                }
                if card.archived {
                    return Err(PrivateStateError::Invalid(format!(
                        "archived card '{id}' must not appear in ordering"
                    )));
                }
                if &card.column_id != column_id {
                    return Err(PrivateStateError::Invalid(format!(
                        "card '{id}' is ordered under column '{column_id}' but belongs to '{}'",
                        card.column_id
                    )));
                }
            }
        }
        for card in &self.cards {
            if !card.archived && !ordered_ids.contains(&card.id) {
                return Err(PrivateStateError::Invalid(format!(
                    "active card '{}' is missing from ordering",
                    card.id
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiStateFile {
    pub schema_version: u32,
    pub workspace_id: WorkspaceId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_column_id: Option<ColumnId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_card_id: Option<CardId>,
    #[serde(default)]
    pub active_filters: Vec<String>,
}

impl UiStateFile {
    pub fn empty(workspace_id: WorkspaceId) -> Self {
        Self {
            schema_version: PRIVATE_SCHEMA_VERSION,
            workspace_id,
            selected_column_id: None,
            selected_card_id: None,
            active_filters: Vec::new(),
        }
    }

    pub fn validate_for(&self, workspace_id: WorkspaceId) -> Result<(), PrivateStateError> {
        validate_header(self.schema_version, self.workspace_id, workspace_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheFile {
    pub schema_version: u32,
    pub workspace_id: WorkspaceId,
    #[serde(default)]
    pub gitlab_issues: Vec<CachedGitlabIssue>,
}

impl CacheFile {
    pub fn empty(workspace_id: WorkspaceId) -> Self {
        Self {
            schema_version: PRIVATE_SCHEMA_VERSION,
            workspace_id,
            gitlab_issues: Vec::new(),
        }
    }

    pub fn validate_for(&self, workspace_id: WorkspaceId) -> Result<(), PrivateStateError> {
        validate_header(self.schema_version, self.workspace_id, workspace_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CachedGitlabIssue {
    pub provider_id: ProviderId,
    pub project: String,
    pub iid: u64,
    pub title: String,
    pub state: String,
    #[serde(default)]
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncQueueFile {
    pub schema_version: u32,
    pub workspace_id: WorkspaceId,
    #[serde(default)]
    pub operations: Vec<SyncOperation>,
}

impl SyncQueueFile {
    pub fn empty(workspace_id: WorkspaceId) -> Self {
        Self {
            schema_version: PRIVATE_SCHEMA_VERSION,
            workspace_id,
            operations: Vec::new(),
        }
    }

    pub fn validate_for(&self, workspace_id: WorkspaceId) -> Result<(), PrivateStateError> {
        validate_header(self.schema_version, self.workspace_id, workspace_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncOperation {
    pub id: Uuid,
    pub provider_id: ProviderId,
    pub project: String,
    pub issue_iid: u64,
    pub operation: SyncOperationKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SyncOperationKind {
    SetStatus { provider_status: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivateStateError {
    UnsupportedSchemaVersion {
        found: u32,
    },
    WorkspaceMismatch {
        expected: WorkspaceId,
        found: WorkspaceId,
    },
    Invalid(String),
}

impl fmt::Display for PrivateStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion { found } => write!(
                formatter,
                "unsupported private state schema version {found}; expected {PRIVATE_SCHEMA_VERSION}"
            ),
            Self::WorkspaceMismatch { expected, found } => write!(
                formatter,
                "private state belongs to workspace '{found}', expected '{expected}'"
            ),
            Self::Invalid(message) => formatter.write_str(message),
        }
    }
}

impl Error for PrivateStateError {}

fn validate_header(
    schema_version: u32,
    found_workspace_id: WorkspaceId,
    expected_workspace_id: WorkspaceId,
) -> Result<(), PrivateStateError> {
    if schema_version != PRIVATE_SCHEMA_VERSION {
        return Err(PrivateStateError::UnsupportedSchemaVersion {
            found: schema_version,
        });
    }
    if found_workspace_id != expected_workspace_id {
        return Err(PrivateStateError::WorkspaceMismatch {
            expected: expected_workspace_id,
            found: found_workspace_id,
        });
    }
    Ok(())
}
