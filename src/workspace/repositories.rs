use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::{RepositoryConfig, RepositoryId, WorkspaceConfig};

use super::WorkspaceError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryAvailability {
    Available { id: RepositoryId, path: PathBuf },
    Missing { id: RepositoryId, path: PathBuf },
}

pub fn inspect_repositories(
    root: &Path,
    workspace: &WorkspaceConfig,
) -> Vec<RepositoryAvailability> {
    workspace
        .repositories
        .iter()
        .map(|repository| {
            let path = resolve_repository_path(root, &repository.path);
            if path.is_dir() {
                RepositoryAvailability::Available {
                    id: repository.id.clone(),
                    path,
                }
            } else {
                RepositoryAvailability::Missing {
                    id: repository.id.clone(),
                    path,
                }
            }
        })
        .collect()
}

pub fn suggested_workspace_root(start: &Path) -> Result<PathBuf, WorkspaceError> {
    let start = if start.is_file() {
        start.parent().unwrap_or(start)
    } else {
        start
    };
    let start = fs::canonicalize(start)
        .map_err(|source| WorkspaceError::io("canonicalize start directory", start, source))?;

    for ancestor in start.ancestors() {
        if is_git_repository(ancestor) {
            return Ok(ancestor.to_owned());
        }
    }
    Ok(start)
}

pub(crate) fn detect_repositories(root: &Path) -> Result<Vec<RepositoryConfig>, WorkspaceError> {
    let mut repositories = Vec::new();
    collect_repository_roots(root, &mut repositories)?;
    repositories.sort();

    let mut used_ids = HashSet::new();
    repositories
        .into_iter()
        .map(|repository_root| {
            let path = repository_root
                .strip_prefix(root)
                .map_err(|_| WorkspaceError::InvalidRoot(repository_root.clone()))?;
            let path_string = if path.as_os_str().is_empty() {
                ".".to_owned()
            } else {
                format!("./{}", path.to_string_lossy().replace('\\', "/"))
            };
            let base_name = repository_root
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("repository");
            let base_id = slugify(base_name);
            let mut id = base_id.clone();
            let mut suffix = 2;
            while !used_ids.insert(id.clone()) {
                id = format!("{base_id}-{suffix}");
                suffix += 1;
            }
            Ok(RepositoryConfig {
                id: RepositoryId::new(id),
                path: path_string,
            })
        })
        .collect()
}

fn collect_repository_roots(
    directory: &Path,
    repositories: &mut Vec<PathBuf>,
) -> Result<(), WorkspaceError> {
    if is_git_repository(directory) {
        repositories.push(directory.to_owned());
        return Ok(());
    }

    let entries = fs::read_dir(directory)
        .map_err(|source| WorkspaceError::io("read workspace directory", directory, source))?;
    for entry in entries {
        let entry = entry
            .map_err(|source| WorkspaceError::io("read workspace directory", directory, source))?;
        let file_type = entry.file_type().map_err(|source| {
            WorkspaceError::io("inspect workspace entry", entry.path(), source)
        })?;
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name();
        if matches!(
            name.to_str(),
            Some(".git" | ".kodeck" | "target" | "node_modules")
        ) {
            continue;
        }
        collect_repository_roots(&entry.path(), repositories)?;
    }
    Ok(())
}

fn is_git_repository(path: &Path) -> bool {
    path.join(".git").exists()
}

fn resolve_repository_path(root: &Path, configured_path: &str) -> PathBuf {
    let relative = configured_path
        .strip_prefix("./")
        .unwrap_or(configured_path);
    root.join(relative)
}

fn slugify(value: &str) -> String {
    let mut result = String::new();
    let mut previous_separator = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            result.push(character.to_ascii_lowercase());
            previous_separator = false;
        } else if !previous_separator && !result.is_empty() {
            result.push('-');
            previous_separator = true;
        }
    }
    while result.ends_with('-') {
        result.pop();
    }
    if result.is_empty() {
        "repository".to_owned()
    } else {
        result
    }
}
