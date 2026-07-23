use std::path::{Path, PathBuf};

use color_eyre::eyre::Result;
use crossterm::event::{self, KeyCode};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::{DefaultTerminal, Frame};
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

use crate::credentials::credential_warnings;
use crate::domain::KnownWorkspace;
use crate::scan::run_workspace_scan;
use crate::storage::AppPaths;
use crate::workspace::{
    DiscoveryOutcome, WorkspaceContext, WorkspaceInitializer, WorkspaceManager,
};

use super::App;

pub fn run(terminal: &mut DefaultTerminal, paths: AppPaths, start: &Path) -> Result<()> {
    let manager = WorkspaceManager::new(paths.clone());
    let context = match manager.discover(start)? {
        DiscoveryOutcome::Found(context) => *context,
        DiscoveryOutcome::SelectionRequired(workspaces) => {
            let Some(workspace) = select_workspace(terminal, &workspaces)? else {
                return Ok(());
            };
            manager.open_root(&workspace.path)?
        }
        DiscoveryOutcome::NotFound { suggested_root } => {
            let initializer = WorkspaceInitializer::new(paths);
            let Some(context) = initialize_workspace(terminal, &initializer, suggested_root)?
            else {
                return Ok(());
            };
            context
        }
    };

    let findings = run_workspace_scan(&context);
    let mut context = context;
    context
        .warnings
        .extend(credential_warnings(&context.config));
    App::from_workspace(context, findings.len()).run(terminal)
}

struct InitializationForm {
    root: PathBuf,
    name: Input,
    error: Option<String>,
}

impl InitializationForm {
    fn new(root: PathBuf) -> Self {
        let default_name = root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Workspace")
            .to_owned();
        Self {
            root,
            name: Input::from(default_name),
            error: None,
        }
    }

    fn submit(
        &mut self,
        initializer: &WorkspaceInitializer,
    ) -> Option<Result<WorkspaceContext, crate::workspace::WorkspaceError>> {
        let name = self.name.value().trim();
        if name.is_empty() {
            self.error = Some("Workspace name must not be empty".to_owned());
            return None;
        }
        Some(initializer.initialize(&self.root, name))
    }

    fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let popup = area.centered(
            Constraint::Percentage(70),
            Constraint::Length(11.min(area.height)),
        );
        frame.render_widget(Clear, popup);
        let block = Block::default()
            .title("Create workspace")
            .borders(Borders::ALL);
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let rows = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Fill(1),
        ])
        .split(inner);
        frame.render_widget(
            Paragraph::new(self.root.display().to_string()).block(Block::default().title("Root")),
            rows[0],
        );
        frame.render_widget(
            Paragraph::new(self.name.value())
                .style(Color::Yellow)
                .block(Block::default().title("Name").borders(Borders::ALL)),
            rows[1],
        );
        if let Some(error) = &self.error {
            frame.render_widget(Paragraph::new(error.as_str()).style(Color::Red), rows[2]);
        }
        frame.render_widget(
            Paragraph::new("Enter  Create    Esc  Quit")
                .style(Style::default().fg(Color::DarkGray)),
            rows[3],
        );

        let scroll = self.name.visual_scroll((rows[1].width.max(3) - 3) as usize);
        let x = self.name.visual_cursor().max(scroll) - scroll + 1;
        frame.set_cursor_position((rows[1].x + x as u16, rows[1].y + 1));
    }
}

fn initialize_workspace(
    terminal: &mut DefaultTerminal,
    initializer: &WorkspaceInitializer,
    root: PathBuf,
) -> Result<Option<WorkspaceContext>> {
    let mut form = InitializationForm::new(root);
    loop {
        terminal.draw(|frame| form.render(frame))?;
        let event = event::read()?;
        let Some(key) = event.as_key_press_event() else {
            continue;
        };
        match key.code {
            KeyCode::Esc => return Ok(None),
            KeyCode::Enter => {
                if let Some(result) = form.submit(initializer) {
                    match result {
                        Ok(context) => return Ok(Some(context)),
                        Err(error) => form.error = Some(error.to_string()),
                    }
                }
            }
            _ => {
                form.name.handle_event(&event);
                form.error = None;
            }
        }
    }
}

fn select_workspace(
    terminal: &mut DefaultTerminal,
    workspaces: &[KnownWorkspace],
) -> Result<Option<KnownWorkspace>> {
    let mut selected = 0;
    loop {
        terminal.draw(|frame| {
            let area = frame.area();
            let items: Vec<_> = workspaces
                .iter()
                .map(|workspace| {
                    ListItem::new(format!("{}  {}", workspace.id, workspace.path.display()))
                })
                .collect();
            let mut state = ListState::default().with_selected(Some(selected));
            frame.render_stateful_widget(
                List::new(items)
                    .block(Block::default().title("Workspaces").borders(Borders::ALL))
                    .highlight_style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                area,
                &mut state,
            );
        })?;

        let Some(key) = event::read()?.as_key_press_event() else {
            continue;
        };
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
            KeyCode::Up => selected = selected.saturating_sub(1),
            KeyCode::Down => selected = (selected + 1).min(workspaces.len().saturating_sub(1)),
            KeyCode::Enter => return Ok(workspaces.get(selected).cloned()),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("kodeck-bootstrap-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn initialization_form_renders_and_creates_a_workspace() {
        let temporary = TestDirectory::new();
        let root = temporary.0.join("project");
        fs::create_dir_all(&root).unwrap();
        let paths = AppPaths::new(temporary.0.join("config"), temporary.0.join("data"));
        let initializer = WorkspaceInitializer::new(paths);
        let mut form = InitializationForm::new(root.clone());
        form.name = Input::from("Test Workspace".to_owned());

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| form.render(frame)).unwrap();
        let rendered = terminal.backend().to_string();
        assert!(rendered.contains("Create workspace"));
        assert!(rendered.contains("Test Workspace"));

        let context = form.submit(&initializer).unwrap().unwrap();
        assert_eq!(context.config.name, "Test Workspace");
        assert!(context.paths.shared_config_file().is_file());
    }
}
