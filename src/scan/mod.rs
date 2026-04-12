use rayon::prelude::*;
use crate::models::finding::Finding;

pub mod walker;
pub mod searcher;

pub fn run_scan(root: &str) -> Vec<Finding> {
    let regex = searcher::build_regex();
    println!("Scanning directory: {}", root);
    println!("Using regex: {}", regex);
    let paths = walker::collect_paths(root);
    println!("Found {} paths", paths.len());
    for path in paths.clone() {
        println!("Found: {}", path);
    }
    
    paths
        .par_iter()
        .flat_map(|path| searcher::search_file(path, &regex))
        .collect()
}