use regex::Regex;
use crate::models::finding::Finding;

pub fn build_regex() -> Regex {
    let tags = vec!["TODO".to_string(), "FIXME".to_string()].join("|");
    Regex::new(&format!(r"(?i)({tags})[:\s]+(.*)"))
        .expect("Pattern regex invalide")
}

pub fn search_file(path: &str, re: &Regex) -> Vec<Finding> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return vec![];
    };

    content
        .lines()
        .enumerate()
        .filter_map(|(i, line)| {
            let cap = re.captures(line)?;
            Some(Finding {
                file: path.to_string(),
                line: i + 1,
                tag: cap.get(1)?.as_str().to_uppercase(),
                message: cap
                    .get(2)
                    .map_or("", |m| m.as_str())
                    .trim()
                    .to_string(),
            })
        })
        .collect()
}