use std::fs;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

use super::*;
use crate::domain::TaskPriority;
use crate::storage::AppPaths;
use crate::tui::state::TaskFormMode;
use crate::tui::ui::BoardLayout;
use crate::workspace::{WorkspaceInitializer, WorkspaceManager};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("kodeck-app-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn test_app(temporary: &TestDirectory) -> App<'static> {
    let root = temporary.0.join("project");
    fs::create_dir_all(&root).unwrap();
    let paths = AppPaths::new(temporary.0.join("config"), temporary.0.join("data"));
    let context = WorkspaceInitializer::new(paths)
        .initialize(&root, "Test Board")
        .unwrap();
    App::from_workspace(context, 0)
}

fn add_task(app: &mut App<'_>, column: usize, title: &str) -> CardId {
    let order = app.columns[column].tasks.len();
    let task = Task::new(
        title.to_owned(),
        format!("Description for {title}"),
        TaskPriority::MEDIUM,
        order,
    );
    let id = task.id;
    app.columns[column].tasks.push(task);
    id
}

fn press(app: &mut App<'_>, code: KeyCode) {
    assert!(!app.handle_key(KeyEvent::new(code, KeyModifiers::NONE)));
}

fn press_ctrl(app: &mut App<'_>, character: char) {
    assert!(!app.handle_key(KeyEvent::new(
        KeyCode::Char(character),
        KeyModifiers::CONTROL,
    )));
}

#[test]
fn board_mutations_are_reloaded_from_workspace_storage() {
    let temporary = TestDirectory::new();
    let root = temporary.0.join("project");
    fs::create_dir_all(&root).unwrap();
    let paths = AppPaths::new(temporary.0.join("config"), temporary.0.join("data"));
    let context = WorkspaceInitializer::new(paths.clone())
        .initialize(&root, "Test Board")
        .unwrap();
    let mut app = App::from_workspace(context, 0);

    app.focused_column = app
        .columns
        .iter()
        .position(|column| column.id.as_str() == "todo")
        .unwrap();
    app.columns[app.focused_column].tasks.push(Task::new(
        "Persist me".to_owned(),
        "Stored outside Git".to_owned(),
        TaskPriority::HIGH,
        0,
    ));
    app.persist_cards();
    app.move_targets_relative(1);
    app.create_column("Blocked".to_owned());
    drop(app);

    let reloaded = WorkspaceManager::new(paths).open_root(&root).unwrap();
    assert!(
        reloaded
            .config
            .columns
            .iter()
            .any(|column| column.id.as_str() == "blocked")
    );
    let card = &reloaded.private_state.cards.cards[0];
    assert_eq!(card.title, "Persist me");
    assert_eq!(card.column_id.as_str(), "in-progress");
}

#[test]
fn empty_workspace_board_renders_without_panicking() {
    let temporary = TestDirectory::new();
    let root = temporary.0.join("project");
    fs::create_dir_all(&root).unwrap();
    let paths = AppPaths::new(temporary.0.join("config"), temporary.0.join("data"));
    let context = WorkspaceInitializer::new(paths)
        .initialize(&root, "Empty Board")
        .unwrap();
    let mut app = App::from_workspace(context, 0);
    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal.draw(|frame| app.render(frame)).unwrap();

    assert!(terminal.backend().to_string().contains("Empty Board"));
}

#[test]
fn vim_navigation_and_aliases_stop_at_board_boundaries() {
    let temporary = TestDirectory::new();
    let mut app = test_app(&temporary);
    add_task(&mut app, 0, "First");
    add_task(&mut app, 0, "Second");

    press(&mut app, KeyCode::Char('h'));
    press(&mut app, KeyCode::Char('k'));
    assert_eq!((app.focused_column, app.focused_task), (0, 0));

    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Down);
    assert_eq!(app.focused_task, 1);

    press(&mut app, KeyCode::Tab);
    assert_eq!(app.focused_column, 1);
    for _ in 0..10 {
        press(&mut app, KeyCode::Char('l'));
    }
    assert_eq!(app.focused_column, app.columns.len() - 1);
}

#[test]
fn column_reordering_preserves_focus_selection_offsets_and_persists() {
    let temporary = TestDirectory::new();
    let root = temporary.0.join("project");
    fs::create_dir_all(&root).unwrap();
    let paths = AppPaths::new(temporary.0.join("config"), temporary.0.join("data"));
    let context = WorkspaceInitializer::new(paths.clone())
        .initialize(&root, "Reordered Board")
        .unwrap();
    let mut app = App::from_workspace(context, 0);
    let first = add_task(&mut app, 1, "Focused");
    let second = add_task(&mut app, 1, "Keep order");
    let selected = add_task(&mut app, 2, "Selected");
    app.focused_column = 1;
    app.focused_task = 0;
    app.selected_cards.insert(selected);
    app.viewport.card_offsets[0] = 1;
    app.viewport.card_offsets[1] = 3;
    app.persist_cards();
    let original_columns: Vec<_> = app.columns.iter().map(|column| column.id.clone()).collect();

    press(&mut app, KeyCode::Char('['));

    assert_eq!(app.focused_column, 0);
    assert_eq!(app.focused_card_id(), Some(first));
    assert!(app.selected_cards.contains(&selected));
    assert_eq!(app.viewport.card_offsets[0], 3);
    assert_eq!(app.viewport.card_offsets[1], 1);
    assert_eq!(app.columns[0].id, original_columns[1]);
    assert_eq!(app.columns[1].id, original_columns[0]);
    assert_eq!(
        app.columns[0]
            .tasks
            .iter()
            .map(|task| task.id)
            .collect::<Vec<_>>(),
        vec![first, second]
    );
    assert_eq!(
        app.status_message
            .as_ref()
            .map(|message| message.text.as_str()),
        Some("Column moved left")
    );

    let reloaded = WorkspaceManager::new(paths).open_root(&root).unwrap();
    let persisted_columns: Vec<_> = reloaded
        .config
        .columns
        .iter()
        .map(|column| column.id.clone())
        .collect();
    assert_eq!(
        persisted_columns,
        app.columns
            .iter()
            .map(|column| column.id.clone())
            .collect::<Vec<_>>()
    );
    let stored_card = reloaded
        .private_state
        .cards
        .cards
        .iter()
        .find(|card| card.id == first)
        .unwrap();
    assert_eq!(stored_card.column_id, original_columns[1]);

    for width in [80, 160] {
        let backend = TestBackend::new(width, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.render(frame)).unwrap();
        let layout = BoardLayout::compute(Rect::new(0, 0, width, 24), app.columns.len(), false);
        assert!(app.focused_column >= app.viewport.first_column);
        assert!(app.focused_column < app.viewport.first_column + layout.visible_columns);
    }
}

#[test]
fn column_reordering_respects_boundaries_and_works_from_details() {
    let temporary = TestDirectory::new();
    let mut app = test_app(&temporary);
    let card_id = add_task(&mut app, 0, "Follow the column");

    press(&mut app, KeyCode::Char('['));
    assert_eq!(app.focused_column, 0);
    assert_eq!(
        app.status_message
            .as_ref()
            .map(|message| message.text.as_str()),
        Some("Column cannot move further")
    );

    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char(']'));
    assert_eq!(app.focused_column, 1);
    assert_eq!(app.focused_card_id(), Some(card_id));
    assert_eq!(app.detail.map(|detail| detail.card_id), Some(card_id));

    app.focused_column = app.columns.len() - 1;
    press(&mut app, KeyCode::Char(']'));
    assert_eq!(app.focused_column, app.columns.len() - 1);
    assert_eq!(
        app.status_message
            .as_ref()
            .map(|message| message.text.as_str()),
        Some("Column cannot move further")
    );
}

#[test]
fn relative_move_applies_to_each_selected_card_and_clears_selection() {
    let temporary = TestDirectory::new();
    let mut app = test_app(&temporary);
    let first = add_task(&mut app, 0, "Inbox");
    let second = add_task(&mut app, 1, "Todo");
    let last_column = app.columns.len() - 1;
    let boundary = add_task(&mut app, last_column, "Already done");
    app.selected_cards = HashSet::from([first, second, boundary]);

    press(&mut app, KeyCode::Char('L'));

    assert_eq!(app.locate_card(first).unwrap().0, 1);
    assert_eq!(app.locate_card(second).unwrap().0, 2);
    assert_eq!(app.locate_card(boundary).unwrap().0, app.columns.len() - 1);
    assert!(app.selected_cards.is_empty());
}

#[test]
fn move_picker_sends_a_multi_column_selection_to_one_destination() {
    let temporary = TestDirectory::new();
    let mut app = test_app(&temporary);
    let first = add_task(&mut app, 0, "Inbox");
    let second = add_task(&mut app, 1, "Todo");
    app.selected_cards = HashSet::from([first, second]);
    app.focused_column = 1;

    press(&mut app, KeyCode::Char('m'));
    assert!(app.move_picker.is_some());
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('j'));
    let destination = app.move_picker.as_ref().unwrap().selected_column;
    press(&mut app, KeyCode::Enter);

    assert_eq!(app.locate_card(first).unwrap().0, destination);
    assert_eq!(app.locate_card(second).unwrap().0, destination);
    assert!(app.move_picker.is_none());
    assert!(app.selected_cards.is_empty());
}

#[test]
fn archive_requires_confirmation_and_y_accepts_it() {
    let temporary = TestDirectory::new();
    let mut app = test_app(&temporary);
    let card_id = add_task(&mut app, 0, "Keep or archive");

    press(&mut app, KeyCode::Char('x'));
    assert!(app.confirmation.is_some());
    press(&mut app, KeyCode::Enter);
    assert!(app.locate_card(card_id).is_some());

    press(&mut app, KeyCode::Char('x'));
    press(&mut app, KeyCode::Char('y'));
    assert!(app.locate_card(card_id).is_none());
    assert_eq!(app.archived_tasks.last().unwrap().task.id, card_id);
}

#[test]
fn goto_sequences_and_visible_labels_move_focus() {
    let temporary = TestDirectory::new();
    let mut app = test_app(&temporary);
    let first = add_task(&mut app, 0, "First target");
    let last_column = app.columns.len() - 1;
    let last = add_task(&mut app, last_column, "Last target");
    app.focus_card(last);

    press(&mut app, KeyCode::Char('g'));
    assert!(app.goto_pending);
    press(&mut app, KeyCode::Char('g'));
    assert_eq!(app.focused_card_id(), Some(first));

    press(&mut app, KeyCode::Char('g'));
    press(&mut app, KeyCode::Char('e'));
    assert_eq!(app.focused_card_id(), Some(last));
    press(&mut app, KeyCode::Char('g'));
    press(&mut app, KeyCode::Char('h'));
    assert_eq!(app.focused_column, 0);
    press(&mut app, KeyCode::Char('g'));
    press(&mut app, KeyCode::Char('l'));
    assert_eq!(app.focused_column, last_column);

    let backend = TestBackend::new(220, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    press(&mut app, KeyCode::Char('g'));
    press(&mut app, KeyCode::Char('w'));
    let target = app
        .goto_labels
        .as_ref()
        .unwrap()
        .targets
        .last()
        .unwrap()
        .clone();
    for character in target.label.chars() {
        press(&mut app, KeyCode::Char(character));
    }
    assert_eq!(app.focused_card_id(), Some(target.card_id));
    assert!(app.goto_labels.is_none());
}

#[test]
fn ctrl_s_saves_card_forms_and_escape_discards_them() {
    let temporary = TestDirectory::new();
    let mut app = test_app(&temporary);

    press(&mut app, KeyCode::Char('n'));
    for character in "New card".chars() {
        press(&mut app, KeyCode::Char(character));
    }
    press(&mut app, KeyCode::Enter);
    for character in "Useful description".chars() {
        press(&mut app, KeyCode::Char(character));
    }
    press_ctrl(&mut app, 's');

    assert!(app.task_form.is_none());
    assert_eq!(app.columns[0].tasks[0].title, "New card");

    press(&mut app, KeyCode::Char('n'));
    press(&mut app, KeyCode::Char('x'));
    press(&mut app, KeyCode::Esc);
    assert!(app.task_form.is_none());
    assert_eq!(app.columns[0].tasks.len(), 1);
}

#[test]
fn ctrl_s_creates_a_column_and_x_deletes_it_after_confirmation() {
    let temporary = TestDirectory::new();
    let mut app = test_app(&temporary);
    let original_count = app.columns.len();

    press(&mut app, KeyCode::Char('N'));
    for character in "Blocked".chars() {
        press(&mut app, KeyCode::Char(character));
    }
    press_ctrl(&mut app, 's');
    assert_eq!(app.columns.len(), original_count + 1);
    assert_eq!(app.columns.last().unwrap().name, "Blocked");

    app.focused_column = app.columns.len() - 1;
    press(&mut app, KeyCode::Char('X'));
    press(&mut app, KeyCode::Char('h'));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.columns.len(), original_count);
    assert!(app.columns.iter().all(|column| column.name != "Blocked"));
}

#[test]
fn help_goto_and_confirmation_render_on_a_compact_terminal() {
    let temporary = TestDirectory::new();
    let mut app = test_app(&temporary);
    add_task(&mut app, 0, "Compact");
    let backend = TestBackend::new(50, 16);
    let mut terminal = Terminal::new(backend).unwrap();

    press(&mut app, KeyCode::Char('?'));
    terminal.draw(|frame| app.render(frame)).unwrap();
    assert!(
        terminal
            .backend()
            .to_string()
            .contains("Keyboard shortcuts")
    );

    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('g'));
    terminal.draw(|frame| app.render(frame)).unwrap();
    assert!(terminal.backend().to_string().contains("visible card"));

    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('x'));
    terminal.draw(|frame| app.render(frame)).unwrap();
    let rendered = terminal.backend().to_string();
    assert!(rendered.contains("Archive 1 card"));
    assert!(rendered.contains("Compact"));
}

#[test]
fn board_footer_keeps_help_and_quit_visible_at_eighty_columns() {
    let temporary = TestDirectory::new();
    let mut app = test_app(&temporary);
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal.draw(|frame| app.render(frame)).unwrap();

    let rendered = terminal.backend().to_string();
    assert!(rendered.contains("? help"));
    assert!(rendered.contains("q quit"));
}

#[test]
fn complete_column_viewport_tracks_focus_at_supported_widths() {
    let temporary = TestDirectory::new();
    let mut app = test_app(&temporary);
    app.create_column("Blocked".to_owned());
    app.create_column("Ready".to_owned());
    app.create_column("Released".to_owned());
    for column in 0..app.columns.len() {
        add_task(&mut app, column, &format!("Card in column {column}"));
    }
    app.focus_column(app.columns.len() - 1);

    for width in [40, 60, 80, 120, 160] {
        let backend = TestBackend::new(width, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.render(frame)).unwrap();
        let layout = BoardLayout::compute(Rect::new(0, 0, width, 24), app.columns.len(), false);
        if width < 50 {
            assert!(
                terminal
                    .backend()
                    .to_string()
                    .contains("Terminal too small")
            );
        } else {
            assert!(app.focused_column >= app.viewport.first_column);
            assert!(app.focused_column < app.viewport.first_column + layout.visible_columns);
            assert_eq!(app.visible_cards.len(), layout.visible_columns);
        }
    }
}

#[test]
fn vertical_viewport_never_slices_two_line_cards() {
    let temporary = TestDirectory::new();
    let mut app = test_app(&temporary);
    let mut last = None;
    for index in 0..12 {
        last = Some(add_task(
            &mut app,
            0,
            &format!("A long card title number {index} that must truncate"),
        ));
    }
    app.focused_task = 11;
    let backend = TestBackend::new(80, 16);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal.draw(|frame| app.render(frame)).unwrap();

    assert_eq!(app.viewport.card_offsets[0], 7);
    assert_eq!(app.visible_cards.len(), 5);
    assert!(app.visible_cards.contains(&last.unwrap()));
    assert!(terminal.backend().to_string().contains('…'));
}

#[test]
fn details_follow_board_navigation_and_scroll_independently() {
    let temporary = TestDirectory::new();
    let mut app = test_app(&temporary);
    let first = add_task(&mut app, 0, "First");
    let second = add_task(&mut app, 0, "Second");
    app.columns[0].tasks[1].description = (1..=30)
        .map(|line| format!("Description line {line}"))
        .collect::<Vec<_>>()
        .join("\n");

    press(&mut app, KeyCode::Enter);
    assert_eq!(app.detail.unwrap().card_id, first);
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(app.detail.unwrap().card_id, second);
    press_ctrl(&mut app, 'd');
    assert_eq!(app.detail.unwrap().scroll, 4);

    for width in [80, 120, 160] {
        let backend = TestBackend::new(width, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.render(frame)).unwrap();
        let rendered = terminal.backend().to_string();
        assert!(rendered.contains("Details"));
        assert!(rendered.contains("Description line 5"));
    }
    press(&mut app, KeyCode::Esc);
    assert!(app.detail.is_none());
}

#[test]
fn unified_task_form_reports_inline_validation_errors() {
    let temporary = TestDirectory::new();
    let mut app = test_app(&temporary);

    press(&mut app, KeyCode::Char('n'));
    press_ctrl(&mut app, 's');
    assert_eq!(
        app.task_form.as_ref().unwrap().error.as_deref(),
        Some("Title is required")
    );
    for character in "Only a title".chars() {
        press(&mut app, KeyCode::Char(character));
    }
    press_ctrl(&mut app, 's');
    assert_eq!(
        app.task_form.as_ref().unwrap().error.as_deref(),
        Some("Description is required")
    );
    press(&mut app, KeyCode::Esc);
    assert!(app.task_form.is_none());
}

#[test]
fn unified_edit_form_updates_the_card_identified_by_id_and_persists_it() {
    let temporary = TestDirectory::new();
    let mut app = test_app(&temporary);
    let card_id = add_task(&mut app, 0, "Before");
    app.focused_task = 0;

    press(&mut app, KeyCode::Char('e'));
    let form = app.task_form.as_mut().unwrap();
    assert_eq!(form.mode, TaskFormMode::Edit(card_id));
    form.title = tui_input::Input::from("After".to_owned());
    form.description = ratatui_textarea::TextArea::from(["Persisted description"]);
    press_ctrl(&mut app, 's');

    assert!(app.task_form.is_none());
    assert_eq!(app.columns[0].tasks[0].title, "After");
    let stored = app.private_store.load_cards().unwrap();
    let stored_card = stored.cards.iter().find(|card| card.id == card_id).unwrap();
    assert_eq!(stored_card.title, "After");
    assert_eq!(stored_card.description, "Persisted description");
}
