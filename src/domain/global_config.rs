use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::WorkspaceId;

pub const GLOBAL_CONFIG_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlobalConfig {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_editor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_workspace_id: Option<WorkspaceId>,
    #[serde(default)]
    pub known_workspaces: Vec<KnownWorkspace>,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            schema_version: GLOBAL_CONFIG_SCHEMA_VERSION,
            default_editor: None,
            last_workspace_id: None,
            known_workspaces: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownWorkspace {
    pub id: WorkspaceId,
    pub path: PathBuf,
}
