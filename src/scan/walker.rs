use ignore::WalkBuilder;

pub fn collect_paths(root: &str) -> Vec<String> {
    let excluded_extensions: Vec<String> = vec![];

    WalkBuilder::new(root)
        .git_ignore(true)
        .hidden(false)
        .filter_entry(move |e| {
            let name = e.file_name().to_string_lossy();
            !matches!(
                name.as_ref(),
                ".git" | ".kodeck" | "target" | "node_modules"
            ) && !excluded_extensions.contains(&e.path().to_string_lossy().to_string())
        })
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_some_and(|file_type| file_type.is_file()))
        .map(|e| e.path().to_string_lossy().to_string())
        .collect()
}
