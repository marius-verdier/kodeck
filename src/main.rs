mod app;
mod models;
mod scan;

use std::thread::current;
use color_eyre::Result;
use crate::app::App;

fn main() -> Result<()> {
    color_eyre::install()?;
    // ratatui::run(|terminal| {App::new().run(terminal)})

    let mut findings = scan::run_scan(".");

    if findings.is_empty() {
        println!("Aucune annotation trouvée.");
        return Ok(());
    }

    findings.sort_by(|a, b| {
        a.file.cmp(&b.file)
            .then(a.tag.cmp(&b.tag))
            .then(a.line.cmp(&b.line))
    });

    let mut current_file = String::new();
    for f in &findings {
        if f.file != current_file {
            println!("\n── {} ──", f.tag);
            current_file = f.file.clone();
        }
        println!("  {}:{} — {}", f.tag, f.line, f.message);
    }

    println!("\n{} annotation(s) trouvée(s)", findings.len());
    Ok(())
}