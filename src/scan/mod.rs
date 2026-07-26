use rayon::prelude::*;
use std::path::Path;

use crate::domain::{AnnotationTagMapping, Finding, RepositoryId, WorkspaceConfig};
use crate::workspace::{RepositoryAvailability, WorkspaceContext, inspect_repositories};

mod reconcile;
pub mod searcher;
pub mod walker;

pub fn run_scan(root: &str) -> Vec<Finding> {
    scan_repository(
        Path::new(root),
        &RepositoryId::from("repository"),
        &AnnotationTagMapping::defaults(),
    )
}

pub fn scan_workspace(root: &Path, config: &WorkspaceConfig) -> Vec<Finding> {
    inspect_repositories(root, config)
        .into_par_iter()
        .filter_map(|repository| match repository {
            RepositoryAvailability::Available { id, path } => Some((id, path)),
            RepositoryAvailability::Missing { .. } => None,
        })
        .flat_map(|(id, path)| scan_repository(&path, &id, &config.annotation_tags))
        .collect()
}

fn scan_repository(
    root: &Path,
    repository_id: &RepositoryId,
    mappings: &[AnnotationTagMapping],
) -> Vec<Finding> {
    let tags: Vec<_> = mappings.iter().map(|mapping| mapping.tag.clone()).collect();
    let Some(regex) = searcher::build_regex(&tags) else {
        return Vec::new();
    };
    walker::collect_paths(&root.to_string_lossy())
        .par_iter()
        .flat_map(|file| {
            let file = Path::new(file);
            let relative = file
                .strip_prefix(root)
                .unwrap_or(file)
                .to_string_lossy()
                .replace('\\', "/");
            searcher::search_file(file, &relative, repository_id, &regex)
        })
        .collect()
}

pub fn run_workspace_scan(context: &WorkspaceContext) -> Vec<Finding> {
    scan_workspace(&context.root, &context.config)
}

pub use reconcile::{AnnotationSyncSummary, reconcile_annotations};
