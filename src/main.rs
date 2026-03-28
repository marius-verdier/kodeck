mod app;
mod models;

use color_eyre::Result;
use crate::app::App;

fn main() -> Result<()> {
    color_eyre::install()?;
    ratatui::run(|terminal| {App::new().run(terminal)})
}