use std::collections::HashSet;
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{ProviderConfig, ProviderId, TaskPriority};

pub const WORKSPACE_SCHEMA_VERSION: u32 = 1;

const DEFAULT_COLUMNS: [(&str, &str); 5] = [
    ("inbox", "Inbox"),
    ("todo", "To do"),
    ("in-progress", "In progress"),
    ("review", "Review"),
    ("done", "Done"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkspaceId(Uuid);

impl WorkspaceId {
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

impl fmt::Display for WorkspaceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

macro_rules! string_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self::new(value)
            }
        }
    };
}

string_id!(RepositoryId);
string_id!(ColumnId);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryConfig {
    pub id: RepositoryId,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnConfig {
    pub id: ColumnId,
    pub name: String,
}

impl ColumnConfig {
    pub fn defaults() -> Vec<Self> {
        DEFAULT_COLUMNS
            .into_iter()
            .map(|(id, name)| Self {
                id: ColumnId::from(id),
                name: name.to_owned(),
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusMapping {
    pub provider_id: ProviderId,
    pub provider_status: String,
    pub column_id: ColumnId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnotationTagMapping {
    pub tag: String,
    pub column_id: ColumnId,
    pub priority: TaskPriority,
}

impl AnnotationTagMapping {
    pub fn defaults() -> Vec<Self> {
        [
            ("TODO", TaskPriority::LOW),
            ("FIXME", TaskPriority::HIGH),
            ("BUG", TaskPriority::HIGH),
        ]
        .into_iter()
        .map(|(tag, priority)| Self {
            tag: tag.to_owned(),
            column_id: ColumnId::from("inbox"),
            priority,
        })
        .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceConfig {
    pub schema_version: u32,
    pub id: WorkspaceId,
    pub name: String,
    #[serde(default)]
    pub repositories: Vec<RepositoryConfig>,
    pub columns: Vec<ColumnConfig>,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    #[serde(default)]
    pub status_mappings: Vec<StatusMapping>,
    #[serde(default = "AnnotationTagMapping::defaults")]
    pub annotation_tags: Vec<AnnotationTagMapping>,
}

impl WorkspaceConfig {
    pub fn new(id: WorkspaceId, name: impl Into<String>) -> Self {
        Self {
            schema_version: WORKSPACE_SCHEMA_VERSION,
            id,
            name: name.into(),
            repositories: Vec::new(),
            columns: ColumnConfig::defaults(),
            providers: Vec::new(),
            status_mappings: Vec::new(),
            annotation_tags: AnnotationTagMapping::defaults(),
        }
    }

    pub fn validate(&self) -> Result<(), WorkspaceValidationErrors> {
        let mut errors = Vec::new();

        if self.schema_version != WORKSPACE_SCHEMA_VERSION {
            push_issue(
                &mut errors,
                "schema_version",
                format!(
                    "unsupported schema version {}; expected {}",
                    self.schema_version, WORKSPACE_SCHEMA_VERSION
                ),
            );
        }
        if self.id.as_uuid().is_nil() {
            push_issue(&mut errors, "id", "workspace UUID must not be nil");
        }
        validate_name(&mut errors, "name", &self.name, "workspace name");

        let mut repository_ids = HashSet::new();
        let mut repository_paths = HashSet::new();
        for (index, repository) in self.repositories.iter().enumerate() {
            let path = format!("repositories[{index}]");
            validate_id(
                &mut errors,
                &format!("{path}.id"),
                repository.id.as_str(),
                "repository id",
            );
            if !repository_ids.insert(repository.id.as_str()) {
                push_issue(
                    &mut errors,
                    format!("{path}.id"),
                    format!("duplicate repository id '{}'", repository.id),
                );
            }

            match normalize_repository_path(&repository.path) {
                Ok(normalized) => {
                    if !repository_paths.insert(normalized) {
                        push_issue(
                            &mut errors,
                            format!("{path}.path"),
                            format!("duplicate repository path '{}'", repository.path),
                        );
                    }
                }
                Err(message) => {
                    push_issue(&mut errors, format!("{path}.path"), message);
                }
            }
        }

        if self.columns.is_empty() {
            push_issue(&mut errors, "columns", "at least one column is required");
        }
        let mut column_ids = HashSet::new();
        for (index, column) in self.columns.iter().enumerate() {
            let path = format!("columns[{index}]");
            validate_id(
                &mut errors,
                &format!("{path}.id"),
                column.id.as_str(),
                "column id",
            );
            validate_name(
                &mut errors,
                &format!("{path}.name"),
                &column.name,
                "column name",
            );
            if !column_ids.insert(column.id.as_str()) {
                push_issue(
                    &mut errors,
                    format!("{path}.id"),
                    format!("duplicate column id '{}'", column.id),
                );
            }
        }

        let mut provider_ids = HashSet::new();
        for (index, provider) in self.providers.iter().enumerate() {
            let path = format!("providers[{index}]");
            validate_id(
                &mut errors,
                &format!("{path}.id"),
                provider.id().as_str(),
                "provider id",
            );
            if !provider_ids.insert(provider.id().as_str()) {
                push_issue(
                    &mut errors,
                    format!("{path}.id"),
                    format!("duplicate provider id '{}'", provider.id()),
                );
            }
            validate_gitlab_provider(&mut errors, &path, provider);
        }

        let mut mapped_statuses = HashSet::new();
        for (index, mapping) in self.status_mappings.iter().enumerate() {
            let path = format!("status_mappings[{index}]");
            validate_id(
                &mut errors,
                &format!("{path}.provider_id"),
                mapping.provider_id.as_str(),
                "provider id",
            );
            validate_id(
                &mut errors,
                &format!("{path}.column_id"),
                mapping.column_id.as_str(),
                "column id",
            );
            validate_name(
                &mut errors,
                &format!("{path}.provider_status"),
                &mapping.provider_status,
                "provider status",
            );

            if !provider_ids.contains(mapping.provider_id.as_str()) {
                push_issue(
                    &mut errors,
                    format!("{path}.provider_id"),
                    format!("unknown provider id '{}'", mapping.provider_id),
                );
            }
            if !column_ids.contains(mapping.column_id.as_str()) {
                push_issue(
                    &mut errors,
                    format!("{path}.column_id"),
                    format!("unknown column id '{}'", mapping.column_id),
                );
            }
            if !mapped_statuses.insert((
                mapping.provider_id.as_str(),
                mapping.provider_status.as_str(),
            )) {
                push_issue(
                    &mut errors,
                    path,
                    format!(
                        "provider status '{}' is mapped more than once for '{}'",
                        mapping.provider_status, mapping.provider_id
                    ),
                );
            }
        }

        let mut annotation_tags = HashSet::new();
        for (index, mapping) in self.annotation_tags.iter().enumerate() {
            let path = format!("annotation_tags[{index}]");
            let normalized = mapping.tag.to_ascii_uppercase();
            if mapping.tag.is_empty()
                || !mapping.tag.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
                })
            {
                push_issue(
                    &mut errors,
                    format!("{path}.tag"),
                    "annotation tag must contain only ASCII letters, digits, '_' or '-'",
                );
            } else if !annotation_tags.insert(normalized) {
                push_issue(
                    &mut errors,
                    format!("{path}.tag"),
                    format!("duplicate annotation tag '{}'", mapping.tag),
                );
            }
            if !column_ids.contains(mapping.column_id.as_str()) {
                push_issue(
                    &mut errors,
                    format!("{path}.column_id"),
                    format!("unknown column id '{}'", mapping.column_id),
                );
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(WorkspaceValidationErrors::new(errors))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceValidationIssue {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceValidationErrors {
    issues: Vec<WorkspaceValidationIssue>,
}

impl WorkspaceValidationErrors {
    fn new(issues: Vec<WorkspaceValidationIssue>) -> Self {
        Self { issues }
    }

    pub fn issues(&self) -> &[WorkspaceValidationIssue] {
        &self.issues
    }
}

impl fmt::Display for WorkspaceValidationErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "workspace configuration has {} validation error(s)",
            self.issues.len()
        )?;
        for issue in &self.issues {
            write!(formatter, "; {}: {}", issue.path, issue.message)?;
        }
        Ok(())
    }
}

impl Error for WorkspaceValidationErrors {}

fn push_issue(
    errors: &mut Vec<WorkspaceValidationIssue>,
    path: impl Into<String>,
    message: impl Into<String>,
) {
    errors.push(WorkspaceValidationIssue {
        path: path.into(),
        message: message.into(),
    });
}

fn validate_id(errors: &mut Vec<WorkspaceValidationIssue>, path: &str, value: &str, label: &str) {
    let mut characters = value.chars();
    let valid_first = characters
        .next()
        .is_some_and(|character| character.is_ascii_alphanumeric());
    let valid_rest = characters
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'));

    if !valid_first || !valid_rest {
        push_issue(
            errors,
            path,
            format!(
                "{label} must start with an ASCII letter or digit and contain only letters, digits, '-', '_' or '.'"
            ),
        );
    }
}

fn validate_name(errors: &mut Vec<WorkspaceValidationIssue>, path: &str, value: &str, label: &str) {
    if value.trim().is_empty() {
        push_issue(errors, path, format!("{label} must not be empty"));
    } else if value.trim() != value {
        push_issue(
            errors,
            path,
            format!("{label} must not have leading or trailing whitespace"),
        );
    }
}

fn normalize_repository_path(path: &str) -> Result<String, &'static str> {
    if path.is_empty() {
        return Err("repository path must not be empty");
    }
    if path.contains('\\') {
        return Err("repository path must use forward slashes");
    }
    if path.starts_with('/')
        || path
            .as_bytes()
            .get(1)
            .is_some_and(|separator| *separator == b':')
    {
        return Err("repository path must be relative to the workspace root");
    }
    if path.chars().any(char::is_control) {
        return Err("repository path must not contain control characters");
    }

    let mut components = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." if components.pop().is_none() => {
                return Err("repository path must not escape the workspace root");
            }
            ".." => {}
            value => components.push(value),
        }
    }

    Ok(if components.is_empty() {
        ".".to_owned()
    } else {
        components.join("/")
    })
}

fn validate_gitlab_provider(
    errors: &mut Vec<WorkspaceValidationIssue>,
    path: &str,
    provider: &ProviderConfig,
) {
    let instance_url = provider.instance_url();
    let authority = instance_url
        .strip_prefix("https://")
        .or_else(|| instance_url.strip_prefix("http://"))
        .and_then(|remainder| remainder.split('/').next());

    if authority.is_none_or(|value| value.is_empty())
        || authority.is_some_and(|value| value.contains('@'))
        || instance_url.chars().any(char::is_whitespace)
        || instance_url.contains(['?', '#'])
    {
        push_issue(
            errors,
            format!("{path}.instance_url"),
            "GitLab instance URL must be an HTTP(S) base URL without credentials, query or fragment",
        );
    }

    let mut projects = HashSet::new();
    for (project_index, project) in provider.projects().iter().enumerate() {
        let project_path = format!("{path}.projects[{project_index}]");
        validate_name(errors, &project_path, project, "GitLab project path");
        if !projects.insert(project.as_str()) {
            push_issue(
                errors,
                project_path,
                format!("duplicate GitLab project '{project}'"),
            );
        }
    }
}
