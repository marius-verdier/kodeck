use regex::Regex;

use crate::domain::{Finding, RepositoryId};

pub fn build_regex(tags: &[String]) -> Option<Regex> {
    if tags.is_empty() {
        return None;
    }
    let mut escaped: Vec<_> = tags.iter().map(|tag| regex::escape(tag)).collect();
    escaped.sort_by_key(|tag| std::cmp::Reverse(tag.len()));
    let tags = escaped.join("|");
    Regex::new(&format!(
        r"(?i)(?:^|[^A-Za-z0-9_])({tags})(?:\s*:\s*|\s+)(\S.*)"
    ))
    .ok()
}

// TODO : Test scan
pub fn search_file(
    file: &std::path::Path,
    relative_path: &str,
    repository_id: &RepositoryId,
    re: &Regex,
) -> Vec<Finding> {
    let Ok(content) = std::fs::read_to_string(file) else {
        return vec![];
    };

    content
        .lines()
        .enumerate()
        .filter_map(|(i, line)| {
            let cap = re.captures(line)?;
            Some(Finding {
                repository_id: repository_id.clone(),
                path: relative_path.to_owned(),
                line: i + 1,
                tag: cap.get(1)?.as_str().to_uppercase(),
                message: cap.get(2)?.as_str().trim().to_owned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn configured_tags_are_case_insensitive_and_regex_escaped() {
        let directory =
            std::env::temp_dir().join(format!("kodeck-search-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let file = directory.join("main.rs");
        fs::write(
            &file,
            "// todo: first\n// BUG+ second\n// TODO:\n// OTHER: no\n",
        )
        .unwrap();
        let regex = build_regex(&["TODO".to_owned(), "BUG+".to_owned()]).unwrap();

        let findings = search_file(&file, "main.rs", &RepositoryId::from("repo"), &regex);

        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].tag, "TODO");
        assert_eq!(findings[1].tag, "BUG+");
        let _ = fs::remove_dir_all(directory);
    }
}
