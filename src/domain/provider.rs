use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderId(String);

impl ProviderId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<&str> for ProviderId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProviderConfig {
    Gitlab {
        id: ProviderId,
        instance_url: String,
        #[serde(default)]
        projects: Vec<String>,
    },
}

impl ProviderConfig {
    pub fn id(&self) -> &ProviderId {
        match self {
            Self::Gitlab { id, .. } => id,
        }
    }

    pub fn instance_url(&self) -> &str {
        match self {
            Self::Gitlab { instance_url, .. } => instance_url,
        }
    }

    pub fn projects(&self) -> &[String] {
        match self {
            Self::Gitlab { projects, .. } => projects,
        }
    }
}
