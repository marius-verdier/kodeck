use color_eyre::Result;

fn main() -> Result<()> {
    color_eyre::install()?;
    let paths = kodeck::storage::AppPaths::discover()?;
    let start = std::env::current_dir()?;
    ratatui::run(|terminal| kodeck::tui::run(terminal, paths, &start))
}
