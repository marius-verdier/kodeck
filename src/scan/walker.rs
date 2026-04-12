use ignore::WalkBuilder;

pub fn collect_paths(root: &str) -> Vec<String> {
    let excluded_extensions: Vec<String> = vec![];

    WalkBuilder::new(root)
        .git_ignore(true)
        .hidden(true)
        .filter_entry(move |e| {
            !excluded_extensions.contains(&e.path().to_string_lossy().to_string())
        })
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map_or(false, |ft| ft.is_file()))
        .map(|e| e.path().to_string_lossy().to_string())
        .collect()
}
