use std::path::{Path, PathBuf};

use color_eyre::eyre::Result;
use crossterm::event::{self, KeyCode};
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

use crate::credentials::credential_warnings;
use crate::domain::KnownWorkspace;
use crate::storage::AppPaths;
use crate::workspace::{
    DiscoveryOutcome, SharedWorkspaceStore, WorkspaceContext, WorkspaceInitializer,
    WorkspaceManager,
};

use super::App;
use super::ui::{MIN_TERMINAL_HEIGHT, MIN_TERMINAL_WIDTH, UiTheme, abbreviated_path, modal_area};

pub fn run(terminal: &mut DefaultTerminal, paths: AppPaths, start: &Path) -> Result<()> {
    let manager = WorkspaceManager::new(paths.clone());
    let context = match manager.discover(start)? {
        DiscoveryOutcome::Found(context) => *context,
        DiscoveryOutcome::SelectionRequired {
            workspaces,
            suggested_root,
        } => {
            let Some(choice) = select_workspace(terminal, &workspaces, suggested_root)? else {
                return Ok(());
            };
            match choice {
                WorkspaceChoice::Initialize { root } => {
                    let initializer = WorkspaceInitializer::new(paths.clone());
                    let Some(context) = initialize_workspace(terminal, &initializer, root)? else {
                        return Ok(());
                    };
                    context
                }
                WorkspaceChoice::Existing { workspace, .. } => {
                    manager.open_root(&workspace.path)?
                }
            }
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

    let mut context = context;
    context
        .warnings
        .extend(credential_warnings(&context.config));
    App::from_workspace(context).run(terminal)
}

struct InitializationForm {
    root: PathBuf,
    name: Input,
    repositories: Vec<String>,
    error: Option<String>,
}

impl InitializationForm {
    fn new(root: PathBuf, initializer: &WorkspaceInitializer) -> Self {
        let default_name = root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Workspace")
            .to_owned();
        Self {
            repositories: initializer
                .detected_repositories(&root)
                .map(|repositories| {
                    repositories
                        .into_iter()
                        .map(|repository| repository.path)
                        .collect()
                })
                .unwrap_or_default(),
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
        if area.width < MIN_TERMINAL_WIDTH || area.height < MIN_TERMINAL_HEIGHT {
            frame.render_widget(
                Paragraph::new("Terminal too small")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::Cyan)),
                area,
            );
            return;
        }

        let theme = UiTheme::default();
        let repository_height = self.repositories.len().clamp(1, 5) as u16;
        let popup = modal_area(area, 72, 12 + repository_height);
        let block = Block::default()
            .title(" Initialize workspace ")
            .borders(Borders::ALL)
            .border_style(theme.focus());
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        let [
            brand,
            root,
            name,
            error,
            repository_title,
            repositories,
            actions,
        ] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(repository_height),
            Constraint::Length(1),
        ])
        .areas(inner);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("KODECK", theme.focus()),
                Span::raw("  Create a focused local board"),
            ])),
            brand,
        );
        frame.render_widget(
            Paragraph::new(abbreviated_path(
                &self.root,
                root.width.saturating_sub(7) as usize,
            ))
            .block(Block::default().title(" Root ")),
            root,
        );
        let name_block = Block::default()
            .title(" Name ")
            .borders(Borders::ALL)
            .border_style(theme.focus());
        let scroll = self
            .name
            .visual_scroll(name.width.saturating_sub(3) as usize);
        frame.render_widget(
            Paragraph::new(self.name.value())
                .scroll((0, scroll as u16))
                .block(name_block),
            name,
        );
        if let Some(message) = &self.error {
            frame.render_widget(
                Paragraph::new(message.as_str()).style(Style::default().fg(theme.error)),
                error,
            );
        }
        frame.render_widget(
            Paragraph::new(format!("Repositories ({})", self.repositories.len()))
                .style(Style::default().fg(theme.secondary)),
            repository_title,
        );
        let repository_text = if self.repositories.is_empty() {
            "  No Git repositories detected".to_owned()
        } else {
            self.repositories
                .iter()
                .map(|repository| format!("  {repository}"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        frame.render_widget(
            Paragraph::new(repository_text)
                .style(Style::default().fg(theme.secondary))
                .wrap(Wrap { trim: false }),
            repositories,
        );
        frame.render_widget(
            Paragraph::new("Enter create  Esc quit").style(Style::default().fg(theme.secondary)),
            actions,
        );
        let x = self.name.visual_cursor().max(scroll) - scroll + 1;
        frame.set_cursor_position((name.x + x as u16, name.y + 1));
    }
}

fn initialize_workspace(
    terminal: &mut DefaultTerminal,
    initializer: &WorkspaceInitializer,
    root: PathBuf,
) -> Result<Option<WorkspaceContext>> {
    let mut form = InitializationForm::new(root, initializer);
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

#[derive(Debug, Clone)]
enum WorkspaceChoice {
    Initialize {
        root: PathBuf,
    },
    Existing {
        workspace: KnownWorkspace,
        name: String,
    },
}

fn load_workspace_choices(
    workspaces: &[KnownWorkspace],
    suggested_root: PathBuf,
) -> Vec<WorkspaceChoice> {
    std::iter::once(WorkspaceChoice::Initialize {
        root: suggested_root,
    })
    .chain(workspaces.iter().cloned().map(|workspace| {
        let name = SharedWorkspaceStore::from_root(&workspace.path)
            .load()
            .map(|config| config.name)
            .unwrap_or_else(|_| "Unavailable workspace".to_owned());
        WorkspaceChoice::Existing { workspace, name }
    }))
    .collect()
}

fn render_workspace_choices(frame: &mut Frame, choices: &[WorkspaceChoice], selected: usize) {
    let area = frame.area();
    if area.width < MIN_TERMINAL_WIDTH || area.height < MIN_TERMINAL_HEIGHT {
        frame.render_widget(
            Paragraph::new("Terminal too small")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Cyan)),
            area,
        );
        return;
    }
    let theme = UiTheme::default();
    let items: Vec<_> = choices
        .iter()
        .map(|choice| {
            let (name, path, metadata) = match choice {
                WorkspaceChoice::Initialize { root } => (
                    "Create workspace here".to_owned(),
                    root,
                    "Initialize a new workspace".to_owned(),
                ),
                WorkspaceChoice::Existing { workspace, name } => {
                    (name.clone(), &workspace.path, workspace.id.to_string())
                }
            };
            ListItem::new(vec![
                Line::styled(name, Style::default().add_modifier(Modifier::BOLD)),
                Line::styled(
                    abbreviated_path(path, area.width.saturating_sub(8) as usize),
                    Style::default().fg(theme.secondary),
                ),
                Line::styled(metadata, Style::default().fg(theme.secondary)),
            ])
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(selected));
    let popup = modal_area(
        area,
        84,
        (choices.len().saturating_mul(3) as u16 + 2).min(area.height.saturating_sub(2)),
    );
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(" KODECK  Select workspace ")
                    .borders(Borders::ALL)
                    .border_style(theme.focus()),
            )
            .highlight_style(theme.focus())
            .highlight_symbol("> "),
        popup,
        &mut state,
    );
}

fn select_workspace(
    terminal: &mut DefaultTerminal,
    workspaces: &[KnownWorkspace],
    suggested_root: PathBuf,
) -> Result<Option<WorkspaceChoice>> {
    let choices = load_workspace_choices(workspaces, suggested_root);
    let mut selected = 0;
    loop {
        terminal.draw(|frame| render_workspace_choices(frame, &choices, selected))?;

        let Some(key) = event::read()?.as_key_press_event() else {
            continue;
        };
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
            KeyCode::Up => selected = selected.saturating_sub(1),
            KeyCode::Down => selected = (selected + 1).min(choices.len().saturating_sub(1)),
            KeyCode::Enter => return Ok(choices.get(selected).cloned()),
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
        fs::create_dir_all(root.join(".git")).unwrap();
        let paths = AppPaths::new(temporary.0.join("config"), temporary.0.join("data"));
        let initializer = WorkspaceInitializer::new(paths);
        let mut form = InitializationForm::new(root.clone(), &initializer);
        form.name = Input::from("Test Workspace".to_owned());

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| form.render(frame)).unwrap();
        let rendered = terminal.backend().to_string();
        assert!(rendered.contains("Initialize workspace"));
        assert!(rendered.contains("Test Workspace"));
        assert!(rendered.contains("Repositories (1)"));

        let context = form.submit(&initializer).unwrap().unwrap();
        assert_eq!(context.config.name, "Test Workspace");
        assert_eq!(context.config.repositories.len(), 1);
        assert!(context.paths.shared_config_file().is_file());
    }

    #[test]
    fn workspace_choices_load_names_from_shared_configuration() {
        let temporary = TestDirectory::new();
        let root = temporary.0.join("named-project");
        let suggested_root = temporary.0.join("new-workspace");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&suggested_root).unwrap();
        let paths = AppPaths::new(temporary.0.join("config"), temporary.0.join("data"));
        let context = WorkspaceInitializer::new(paths)
            .initialize(&root, "Loaded Name")
            .unwrap();
        let known = KnownWorkspace {
            id: context.config.id,
            path: root,
        };

        let choices = load_workspace_choices(&[known], suggested_root.clone());

        assert!(matches!(
            &choices[0],
            WorkspaceChoice::Initialize { root } if root == &suggested_root
        ));
        assert!(matches!(
            &choices[1],
            WorkspaceChoice::Existing { workspace, name }
                if workspace.id == context.config.id && name == "Loaded Name"
        ));

        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_workspace_choices(frame, &choices, 0))
            .unwrap();
        let rendered = terminal.backend().to_string();
        assert!(rendered.contains("Create workspace here"));
        assert!(rendered.contains("Initialize a new workspace"));
        assert!(rendered.contains("Loaded Name"));
    }
}
