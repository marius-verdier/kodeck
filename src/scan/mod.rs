use rayon::prelude::*;

use crate::domain::Finding;
use crate::workspace::{RepositoryAvailability, WorkspaceContext, inspect_repositories};

pub mod searcher;
pub mod walker;

pub fn run_scan(root: &str) -> Vec<Finding> {
    let regex = searcher::build_regex();
    let paths = walker::collect_paths(root);

    paths
        .par_iter()
        .flat_map(|path| searcher::search_file(path, &regex))
        .collect()
}

pub fn run_workspace_scan(context: &WorkspaceContext) -> Vec<Finding> {
    inspect_repositories(&context.root, &context.config)
        .into_par_iter()
        .filter_map(|repository| match repository {
            RepositoryAvailability::Available { path, .. } => Some(path),
            RepositoryAvailability::Missing { .. } => None,
        })
        .flat_map(|path| run_scan(&path.to_string_lossy()))
        .collect()
}
