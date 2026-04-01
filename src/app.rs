use color_eyre::eyre::Result;
use crossterm::event;
use crossterm::event::{KeyCode, KeyEventKind};
use ratatui::{DefaultTerminal, Frame};
use ratatui::layout::{Constraint, HorizontalAlignment, Layout, Margin, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};
use ratatui_textarea::{Input, TextArea};
use ratatui_textarea::Key::Delete;
use tui_input::backend::crossterm::EventHandler;

use crate::models::column::{Column, CreateColumnPopup, DeleteColumnPopup};
use crate::models::task::{CreatingTaskPopup, DisplayingTaskPopup, Task, TaskPriority};

const SCROLL_SIZE: usize = 5;
const COLUMNS_GAP: u16 = 4;
enum InputMode {
    Normal,
    Editing,
    Visual,
}

enum Popup {
    // COLUMN RELATED POPUP
    CreateColumnPopup,
    DeleteColumnPopup,
    // TASK RELATED POPUP
    CreateTaskPopup,
    ShowTaskPopup,
}

enum InputType {
    // COLUMN RELATED INPUTS
    CreatingColumn,
    // TASK RELATED INPUTS
    CreatingTaskTitle,
    CreatingTaskDescription,
    CreatingTaskPriority,
}

pub struct App<'a> {
    input_mode: InputMode,
    active_input: Option<InputType>,
    scroll_x: usize,
    scroll_x_state: ScrollbarState,
    area_width: u16,
    // COLUMNS MANAGEMENT
    columns: Vec<Column>,
    focused_column: usize,
    delete_focused_column: usize,
    create_column_popup: Option<CreateColumnPopup>,
    delete_column_popup: Option<DeleteColumnPopup>,
    // TASKS MANAGEMENT
    focused_task: usize,
    creating_task_popup: Option<CreatingTaskPopup<'a>>,
    displaying_task_popup: Option<DisplayingTaskPopup>,
}

// TODO : Safe delete of column
// TODO : Delete task
// TODO : Done on a task
// TODO : move to specific column by selecting many or one and selecting visually target column
impl App<'_> {
    pub(crate) fn new() -> Self {
        let mut columns = Vec::new();

        let mut first_c = Column::new("TODO".to_string(), 50);
        first_c.tasks.push(Task::new("Task 1".to_string(), String::new(),TaskPriority::LOW, 0));
        first_c.tasks.push(Task::new("Task 2".to_string(), String::new(),TaskPriority::MEDIUM, 1));
        first_c.tasks.push(Task::new("Task 3".to_string(), String::new(),TaskPriority::LOW, 2));
        first_c.tasks.push(Task::new("Task 4".to_string(), String::new(),TaskPriority::HIGH, 3));

        columns.push(first_c);
        columns.push(Column::new("Doing".to_string(), 50));
        columns.push(Column::new("Waiting Review".to_string(), 50));
        columns.push(Column::new("Done".to_string(), 50));

        Self {
            input_mode: InputMode::Normal,
            active_input: None,
            columns,
            scroll_x: 0,
            scroll_x_state: ScrollbarState::new(5),
            area_width: 0,
            focused_column: 0,
            delete_focused_column: 0,
            focused_task: 0,
            create_column_popup: None,
            delete_column_popup: None,
            creating_task_popup: None,
            displaying_task_popup: None,
        }
    }

    pub(crate) fn run(mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        loop {
            terminal.draw(|frame| self.render(frame))?;

            if let Some(key) = event::read()?.as_key_press_event() {
                match self.input_mode {
                    InputMode::Normal if self.displaying_task_popup.is_none() => match key.code {
                        KeyCode::Char('i') => self.input_mode = InputMode::Editing,
                        KeyCode::Char('c') => {
                            self.input_mode = InputMode::Editing;
                            self.toggle_column_creation_popup();
                        }
                        KeyCode::Char('t') => {
                            self.input_mode = InputMode::Editing;
                            self.toggle_task_creation_popup();
                        }
                        KeyCode::Char('D') => {
                            self.delete_focused_column = self.focused_column.clone();
                            self.open_popup(Popup::DeleteColumnPopup);
                        }
                        KeyCode::Char('s') => {
                            self.send_task_to_next();
                        }
                        KeyCode::Char('S') => {
                            self.send_task_to_prev();
                        }
                        KeyCode::Up => {
                            let column = &self.columns[self.focused_column];
                            if column.tasks.is_empty() {
                                continue
                            }
                            self.focused_task = self.focused_task.wrapping_sub(1) % column.tasks.len();
                        }
                        KeyCode::Down => {
                            let column = &self.columns[self.focused_column];
                            if column.tasks.is_empty() {
                                continue
                            }
                            self.focused_task = self.focused_task.wrapping_add(1) % column.tasks.len();
                        }
                        KeyCode::Backspace => {
                            let column = &mut self.columns[self.focused_column];
                            if column.tasks.is_empty() {
                                continue
                            }

                            if self.focused_task >= column.tasks.len() {
                                continue
                            }

                            column.tasks.remove(self.focused_task);
                            if column.tasks.is_empty() {
                                self.focused_task = 0;
                                continue
                            }
                            self.focused_task = self.focused_task.wrapping_sub(1) % column.tasks.len()
                        }
                        KeyCode::Tab => {
                            if let Some(popup) = &mut self.delete_column_popup {
                                popup.delete = !popup.delete;
                                continue
                            }
                            self.focused_column = self.focused_column.saturating_add(1) % self.columns.len();
                            self.focused_task = 0;
                            self.scroll_to_focused_column();
                        }
                        KeyCode::BackTab => {
                            if let Some(popup) = &mut self.delete_column_popup {
                                popup.delete = !popup.delete;
                                continue
                            }
                            self.focused_column = self.focused_column.checked_sub(1).unwrap_or(self.columns.len() - 1);
                            self.focused_task = 0;
                            self.scroll_to_focused_column();
                        }
                        KeyCode::Enter => {
                            if let Some(popup) = &mut self.delete_column_popup {
                                if popup.delete {
                                    self.delete_column(self.delete_focused_column);
                                }
                                self.close_popup(Popup::DeleteColumnPopup);
                                continue
                            }
                        }
                        KeyCode::Esc => {
                            if let Some(popup) = &mut self.delete_column_popup {
                                self.close_popup(Popup::DeleteColumnPopup);
                                continue
                            }
                        }
                        KeyCode::Char(' ') => {
                            self.toggle_task_displaying_popup(self.columns[self.focused_column].tasks[self.focused_task].clone());
                        }
                        KeyCode::Char('q') => return Ok(()),
                        _ => {},
                    }
                    InputMode::Normal => match key.code{
                        KeyCode::Esc => {
                            self.toggle_task_displaying_popup(self.columns[self.focused_column].tasks[self.focused_task].clone());
                        }
                        _ => {}
                    }
                    InputMode::Editing if key.kind == KeyEventKind::Press => match key.code {
                        KeyCode::Esc => {
                            match self.active_input {
                                Some(InputType::CreatingColumn) => {
                                    self.toggle_column_creation_popup()
                                }
                                Some(InputType::CreatingTaskTitle) | Some(InputType::CreatingTaskDescription) | Some(InputType::CreatingTaskPriority) => {
                                    self.toggle_task_creation_popup()
                                }
                                None => {}
                            }
                            self.active_input = None;
                            self.input_mode = InputMode::Normal;
                        }
                        KeyCode::Char(c) => {
                            let event = crossterm::event::Event::Key(key);
                            match &mut self.active_input {
                                Some(InputType::CreatingColumn) => {
                                    if let Some(popup) = &mut self.create_column_popup {
                                        popup.input.handle_event(&event);
                                    }
                                }
                                Some(InputType::CreatingTaskTitle) => {
                                    if let Some(popup) = &mut self.creating_task_popup {
                                        popup.title.handle_event(&event);
                                    }
                                }
                                Some(InputType::CreatingTaskDescription) => {
                                    if let Some(popup) = &mut self.creating_task_popup {
                                        let textarea_event: Input = event::Event::Key(key).into();
                                        popup.description.input(textarea_event);
                                    }
                                }
                                _ => {}
                            }
                        }
                        KeyCode::Backspace => {
                            let event = event::Event::Key(key);
                            match &mut self.active_input {
                                Some(InputType::CreatingColumn) => {
                                    if let Some(popup) = &mut self.create_column_popup {
                                        popup.input.handle_event(&event);
                                    }
                                }
                                Some(InputType::CreatingTaskTitle) => {
                                    if let Some(popup) = &mut self.creating_task_popup {
                                        popup.title.handle_event(&event);
                                    }
                                }
                                Some(InputType::CreatingTaskDescription) => {
                                    if let Some(popup) = &mut self.creating_task_popup {
                                        let textarea_event: Input = event::Event::Key(key).into();
                                        popup.description.input(textarea_event);
                                    }
                                }
                                _ => {}
                            }
                        }
                        KeyCode::Enter => {
                            match self.active_input {
                                Some(InputType::CreatingColumn) => {
                                    if let Some(popup) = &self.create_column_popup {
                                        self.columns.push(Column::new(popup.input.value().to_string(), 50));
                                    }
                                    self.create_column_popup = None;
                                    self.active_input = None;
                                    self.input_mode = InputMode::Normal;
                                }
                                Some(InputType::CreatingTaskTitle) | Some(InputType::CreatingTaskPriority) => {
                                    if self.safe_create_task() { continue; }
                                }
                                Some(InputType::CreatingTaskDescription) => {
                                    if key.modifiers.contains(crossterm::event::KeyModifiers::SHIFT) {
                                        if self.safe_create_task() { continue; }
                                    } else {
                                        if let Some(popup) = &mut self.creating_task_popup {
                                            let textarea_event: Input = event::Event::Key(key).into();
                                            popup.description.input(textarea_event);
                                        }
                                    }
                                }
                                None => {}
                            }
                        }
                        KeyCode::Tab => {
                            match self.active_input {
                                Some(InputType::CreatingTaskTitle) => {
                                    self.active_input = Some(InputType::CreatingTaskDescription);
                                }
                                Some(InputType::CreatingTaskDescription) => {
                                    self.active_input = Some(InputType::CreatingTaskPriority);
                                }
                                Some(InputType::CreatingTaskPriority) => {
                                    self.active_input = Some(InputType::CreatingTaskTitle);
                                }
                                None => {}
                                _ => {}
                            }
                        }
                        KeyCode::BackTab => {
                            match self.active_input {
                                Some(InputType::CreatingTaskTitle) => {
                                    self.active_input = Some(InputType::CreatingTaskPriority);
                                }
                                Some(InputType::CreatingTaskDescription) => {
                                    self.active_input = Some(InputType::CreatingTaskTitle);
                                }
                                Some(InputType::CreatingTaskPriority) => {
                                    self.active_input = Some(InputType::CreatingTaskDescription);
                                }
                                None => {}
                                _ => {}
                            }
                        }
                        KeyCode::Right => {
                            match self.active_input {
                                Some(InputType::CreatingColumn) | Some(InputType::CreatingTaskTitle) | Some(InputType::CreatingTaskDescription) => {
                                    let event = crossterm::event::Event::Key(key);
                                    match &mut self.active_input {
                                        Some(InputType::CreatingColumn) => {
                                            if let Some(popup) = &mut self.create_column_popup { popup.input.handle_event(&event); }
                                        }
                                        Some(InputType::CreatingTaskTitle) => {
                                            if let Some(popup) = &mut self.creating_task_popup { popup.title.handle_event(&event); }
                                        }
                                        Some(InputType::CreatingTaskDescription) => {
                                            let textarea_event: Input = event::Event::Key(key).into();
                                            if let Some(popup) = &mut self.creating_task_popup { popup.description.input(textarea_event); }
                                        }
                                        _ => {}
                                    }
                                }
                                Some(InputType::CreatingTaskPriority) => {
                                    if let Some(popup) = &mut self.creating_task_popup {
                                        popup.priority = match popup.priority {
                                            TaskPriority::LOW => TaskPriority::MEDIUM,
                                            TaskPriority::MEDIUM => TaskPriority::HIGH,
                                            TaskPriority::HIGH => TaskPriority::LOW,
                                        };
                                    }
                                }
                                None => {}
                            }
                        }
                        KeyCode::Left => {
                            match self.active_input {
                                Some(InputType::CreatingColumn) | Some(InputType::CreatingTaskTitle) | Some(InputType::CreatingTaskDescription) => {
                                    let event = crossterm::event::Event::Key(key);
                                    match &mut self.active_input {
                                        Some(InputType::CreatingColumn) => {
                                            if let Some(popup) = &mut self.create_column_popup { popup.input.handle_event(&event); }
                                        }
                                        Some(InputType::CreatingTaskTitle) => {
                                            if let Some(popup) = &mut self.creating_task_popup { popup.title.handle_event(&event); }
                                        }
                                        Some(InputType::CreatingTaskDescription) => {
                                            let textarea_event: Input = event::Event::Key(key).into();
                                            if let Some(popup) = &mut self.creating_task_popup { popup.description.input(textarea_event); }
                                        }
                                        _ => {}
                                    }
                                }
                                Some(InputType::CreatingTaskPriority) => {
                                    if let Some(popup) = &mut self.creating_task_popup {
                                        popup.priority = match popup.priority {
                                            TaskPriority::LOW => TaskPriority::HIGH,
                                            TaskPriority::MEDIUM => TaskPriority::LOW,
                                            TaskPriority::HIGH => TaskPriority::MEDIUM,
                                        };
                                    }
                                }
                                None => {}
                            }
                        }
                        _ => {
                            let event: Input = crossterm::event::Event::Key(key).into();
                            if matches!(self.active_input, Some(InputType::CreatingTaskDescription)) {
                                if let Some(popup) = &mut self.creating_task_popup {
                                    popup.description.input(event);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn open_popup(&mut self, popup: Popup) {
        match popup {
            Popup::DeleteColumnPopup => self.delete_column_popup = Some(DeleteColumnPopup::new()),
            _ => {}
        }
    }

    fn close_popup(&mut self, popup: Popup) {
        match popup {
            Popup::DeleteColumnPopup => self.delete_column_popup = None,
            _ => {}
        }
    }

    fn safe_create_task(&mut self) -> bool {
        if let Some(popup) = &self.creating_task_popup {
            if popup.title.value().is_empty() || popup.description.is_empty() {
                // TODO : DISPLAY ERROR WHEN EMPTY ON CREATION
                return true;
            }

            let task: Task = Task::new(
                popup.title.value().to_string(),
                popup.description.lines().join("\n"),
                popup.priority.clone(),
                self.columns[self.focused_column].tasks.len()
            );
            self.columns[self.focused_column].tasks.push(task);
        }
        self.active_input = None;
        self.input_mode = InputMode::Normal;
        self.creating_task_popup = None;
        false
    }

    fn print_mode(&self) -> String {
        match self.input_mode {
            InputMode::Normal => String::from("Normal"),
            InputMode::Editing => String::from("Editing"),
            InputMode::Visual => String::from("Visual"),
        }
    }

    fn render(&mut self, frame: &mut Frame) {
        let layout = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ]);

        let [help, kanban, status] = frame.area().layout(&layout);

        frame.render_widget(Paragraph::new("Helper text"), help);
        self.render_columns(frame, kanban);
        frame.render_widget(Paragraph::new(format!("Mode is {}", self.print_mode())), status);
    }

    fn render_columns(&mut self, frame: &mut Frame, area: Rect) {
        let content_width = self.compute_total_width();
        self.scroll_x_state = self.scroll_x_state
            .content_length(content_width.saturating_sub(self.area_width as usize))
            .position(self.scroll_x);
        self.area_width = area.width;
        let needs_scroll = content_width > self.area_width as usize;

        let columns_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: if needs_scroll { area.height.saturating_sub(1) } else { area.height},
        };

        let visible_on_the_left = self.scroll_x as u16;
        let visible_on_the_right = (self.scroll_x as u16)  + columns_area.width;

        for i in 0..self.columns.len() {
            let ongoing_column = &self.columns[i];

            let column_left = i as u16 * (ongoing_column.width.clone() as u16 + COLUMNS_GAP);
            let column_rigt = column_left + ongoing_column.width.clone() as u16;

            if column_rigt <= visible_on_the_left || column_left >= visible_on_the_right {
                continue;
            }

            let screen_x = columns_area.x.saturating_add(column_left.saturating_sub(visible_on_the_left));
            let hidden_left = visible_on_the_left.saturating_sub(column_left);
            let visible_width = (ongoing_column.width.clone() as u16).saturating_sub(hidden_left).min(columns_area.right().saturating_sub(screen_x));

            if visible_width <= 0 {
                continue;
            }

            let column = Rect {
                x: screen_x,
                y: columns_area.y,
                width: visible_width,
                height: columns_area.height,
            };

            let ongoing_column = &self.columns[i];

            let block = if i == self.focused_column {
                Block::bordered().title(ongoing_column.name.clone()).border_style(Style::new().bold())
            } else {
                Block::bordered().title(ongoing_column.name.clone())
            };

            let inner = block.inner(column);
            frame.render_widget(block, column);

            let task_constraints: Vec<Constraint> = ongoing_column.tasks
                .iter()
                .map(|_| Constraint::Length(3))
                .collect();

            if !task_constraints.is_empty() {
                let task_areas = Layout::vertical(task_constraints).split(inner);
                for (j, task) in ongoing_column.tasks.iter().enumerate() {
                    let priority_color = match task.priority {
                        TaskPriority::LOW => Color::Blue,
                        TaskPriority::MEDIUM => Color::Yellow,
                        TaskPriority::HIGH => Color::Red,
                    };
                    let task_block = Block::bordered()
                        .border_style(if self.focused_task == j && i == self.focused_column {Style::new().bold()} else {Style::default().fg(priority_color)});
                    let task_inner = task_block.inner(task_areas[j]);
                    frame.render_widget(task_block, task_areas[j]);
                    frame.render_widget(Paragraph::new(task.title.as_str()), task_inner);
                }
            }

        }

        if needs_scroll {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::HorizontalBottom);
            frame.render_stateful_widget(scrollbar, area.inner(Margin {
                horizontal: 1,
                vertical: 0,
            }), &mut self.scroll_x_state);
        }

        self.render_column_creation_popup(frame, area);

        self.render_task_creation_popup(frame, area);

        self.show_task_popup(frame, area);

        self.render_delete_column_popup(frame, area);
    }

    fn render_delete_column_popup(&mut self, frame: &mut Frame, area: Rect) {
        if let Some(popup) = &self.delete_column_popup {
            let block = Block::default()
                .title("Are you sure you want to delete columns?")
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::DarkGray));

            let popup_area = area.centered(Constraint::Percentage(30), Constraint::Length(5));
            let inner_area = block.inner(popup_area);

            frame.render_widget(Clear, popup_area);
            frame.render_widget(block, popup_area);

            let layout = Layout::vertical([
                Constraint::Fill(1),
                Constraint::Length(3),
                Constraint::Fill(1),
            ]);
            let [_, buttons_row, _] = inner_area.layout(&layout);

            let button_layout = Layout::horizontal([
                Constraint::Fill(1),
                Constraint::Length(15),
                Constraint::Length(10),
                Constraint::Length(15),
                Constraint::Fill(1),
            ]);
            let [_, ok_button, _, cancel_button, _] = buttons_row.layout(&button_layout);

            let ok_block = Block::default()
                .borders(Borders::ALL)
                .style(Style::default());

            let cancel_block = Block::default()
                .borders(Borders::ALL)
                .style(Style::default());

            let ok_text_style = if popup.delete {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };

            let cancel_text_style = if !popup.delete {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            let inner_ok = ok_block.inner(ok_button);
            let inner_cancel = cancel_block.inner(cancel_button);

            frame.render_widget(ok_block, ok_button);
            frame.render_widget(cancel_block, cancel_button);


            frame.render_widget(Paragraph::new("Ok").centered().style(ok_text_style), inner_ok);
            frame.render_widget(Paragraph::new("Cancel").centered().style(cancel_text_style), inner_cancel);

        }
    }

    fn render_input(&self, frame: &mut Frame, area: Rect) {
        let width = area.width.max(3) - 3;
        let source_input = match &self.active_input {
            Some(InputType::CreatingColumn) => {
                self.create_column_popup.as_ref().map(|p| &p.input)
            }
            Some(InputType::CreatingTaskTitle) => {
                self.creating_task_popup.as_ref().map(|p| &p.title)
            }
            _ => None,
        };

        if let Some(source_input) = source_input {
            let scroll = source_input.visual_scroll(width as usize);
            let input = Paragraph::new(source_input.value())
                .style(Color::Yellow)
                .scroll((0, scroll as u16))
                .block(Block::bordered().title("Input"));
            frame.render_widget(input, area);

            if matches!(self.input_mode, InputMode::Editing) {
                let x = source_input.visual_cursor().max(scroll) - scroll + 1;
                frame.set_cursor_position((area.x + x as u16, area.y + 1));
            }
        }
    }

    fn render_column_creation_popup(&mut self, frame: &mut Frame, area: Rect) {
        if let Some(popup) = &self.create_column_popup {
            let popup_block = Block::default()
                .title("Creating a new column")
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::DarkGray));

            let area = area.centered(Constraint::Percentage(30), Constraint::Length(3));
            let inner_area = popup_block.inner(area);


            frame.render_widget(Clear, area);
            frame.render_widget(popup_block, area);
            self.render_input(frame, area);
        }
    }

    fn show_task_popup(&mut self, frame: &mut Frame, area: Rect) {
        if let Some(popup) = &self.displaying_task_popup {
            let popup_block = Block::default()
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::DarkGray));

            let description_lines = popup.task.description.clone().lines().count();
            let popup_height = 3 + description_lines.max(1) + 2;

            let popup_area = area.centered(Constraint::Percentage(30), Constraint::Length(popup_height as u16));
            let inner_area = popup_block.inner(popup_area);

            frame.render_widget(Clear, popup_area);

            let layout = Layout::vertical([
                Constraint::Length(4),
                Constraint::Length(popup_height as u16),
            ]);
            let [top_area, description_area] = inner_area.layout(&layout);
            frame.render_widget(popup_block, popup_area);

            let [title_area, priority_area] = top_area.layout(&Layout::horizontal([
                Constraint::Percentage(70),
                Constraint::Percentage(30)
            ]));

            let title_block = Block::default().borders(Borders::NONE);
            let inner_title_area = title_block.inner(title_area);
            frame.render_widget(title_block, title_area);
            frame.render_widget(Paragraph::new(popup.task.title.clone()), inner_title_area);

            let mut priority_block = Block::default().borders(Borders::NONE);
            let inner_priority_area = priority_block.inner(priority_area);
            let priority_style = match popup.task.priority {
                TaskPriority::LOW => Style::default().fg(Color::Blue),
                TaskPriority::MEDIUM => Style::default().fg(Color::Yellow),
                TaskPriority::HIGH => Style::default().fg(Color::Red),
            };

            priority_block = priority_block.border_style(priority_style);
            frame.render_widget(priority_block, priority_area);
            frame.render_widget(Paragraph::new(popup.task.priority.value()).alignment(HorizontalAlignment::Right).style(priority_style), inner_priority_area);

            let description_block = Block::default().borders(Borders::NONE);
            let inner_description_area = description_block.inner(description_area);
            frame.render_widget(description_block, description_area);
            frame.render_widget(Paragraph::new(popup.task.description.clone()).wrap(Wrap::default()), inner_description_area);
        }
    }

    fn render_task_creation_popup(&mut self, frame: &mut Frame, area: Rect) {
        if let Some(t_popup) = &mut self.creating_task_popup {
            let popup_block = Block::default()
                .title("Create a new task")
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::DarkGray));

            let area = area.centered(Constraint::Percentage(60), Constraint::Min(9));
            let inner_area = popup_block.inner(area);

            let layout = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(3),
                Constraint::Length(3),
            ]);
            let [title_area, description_area, priority_area] = inner_area.layout(&layout);

            frame.render_widget(Clear, area);
            frame.render_widget(popup_block, area);

            // Title
            let title_block = Block::default()
                .title("Title")
                .borders(Borders::ALL)
                .border_style(if matches!(self.active_input, Some(InputType::CreatingTaskTitle)) {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                });
            let title_scroll = t_popup.title.visual_scroll((title_area.width.max(3) - 3) as usize);
            frame.render_widget(
                Paragraph::new(t_popup.title.value()).scroll((0, title_scroll as u16)).block(title_block),
                title_area
            );
            if matches!(self.active_input, Some(InputType::CreatingTaskTitle)) {
                let x = t_popup.title.visual_cursor().max(title_scroll) - title_scroll + 1;
                frame.set_cursor_position((title_area.x + x as u16, title_area.y + 1));
            }

            t_popup.description.set_block(
                Block::default()
                    .title("Description")
                    .borders(Borders::ALL)
                    .border_style(if matches!(self.active_input, Some(InputType::CreatingTaskDescription)) {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default()
                    })
            );

            if matches!(self.active_input, Some(InputType::CreatingTaskDescription)) {
                t_popup.description.set_cursor_style(Style::default().add_modifier(ratatui::style::Modifier::REVERSED));
            } else {
                t_popup.description.set_cursor_style(Style::default());
            }

            frame.render_widget(&t_popup.description, description_area);

            let priority_block = Block::default()
                .title("Priority")
                .borders(Borders::ALL)
                .border_style(if matches!(self.active_input, Some(InputType::CreatingTaskPriority)) {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                });
            let inner_priority = priority_block.inner(priority_area);
            frame.render_widget(priority_block, priority_area);

            let priority_layout = Layout::horizontal([
                Constraint::Fill(1),
                Constraint::Fill(1),
                Constraint::Fill(1),
            ]);
            let [low_area, medium_area, high_area] = inner_priority.layout(&priority_layout);

            let low_style = if matches!(t_popup.priority, TaskPriority::LOW) {
                Style::default().bg(Color::Blue)
            } else { Style::default() };
            let medium_style = if matches!(t_popup.priority, TaskPriority::MEDIUM) {
                Style::default().bg(Color::Yellow)
            } else { Style::default() };
            let high_style = if matches!(t_popup.priority, TaskPriority::HIGH) {
                Style::default().bg(Color::Red)
            } else { Style::default() };

            frame.render_widget(Paragraph::new("LOW").style(low_style).alignment(ratatui::layout::Alignment::Center), low_area);
            frame.render_widget(Paragraph::new("MEDIUM").style(medium_style).alignment(ratatui::layout::Alignment::Center), medium_area);
            frame.render_widget(Paragraph::new("HIGH").style(high_style).alignment(ratatui::layout::Alignment::Center), high_area);
        }
    }
    fn toggle_column_creation_popup(&mut self) {
        match self.create_column_popup {
            Some(_) => {
                self.create_column_popup = None;
                self.active_input = None;
            }
            None => {
                self.create_column_popup = Some(CreateColumnPopup::new());
                self.active_input = Some(InputType::CreatingColumn);
            }
        }
    }

    fn toggle_task_displaying_popup(&mut self, task: Task) {
        match self.displaying_task_popup {
            Some(_) => {
                self.displaying_task_popup = None;
            }
            None => {
                self.displaying_task_popup = Some(DisplayingTaskPopup::new(task));
            }
        }
    }

    fn toggle_task_creation_popup(&mut self) {
        match self.creating_task_popup {
            Some(_) => {
                self.creating_task_popup = None;
                self.active_input = None;
            }
            None => {
                self.creating_task_popup = Some(CreatingTaskPopup::new());
                self.active_input = Some(InputType::CreatingTaskTitle)
            }
        }
    }

    fn delete_column(&mut self, column_index: usize) {
        if self.columns.len() > 0 {
            self.columns.remove(column_index);

            if self.focused_column == column_index {
                self.focused_column = self.focused_column.saturating_sub(1);
            }
        }
    }

    fn compute_total_width(&self) -> usize {
        let mut total_width = 0;
        for column in &self.columns {
            total_width += column.width;
        }

        total_width
    }

    fn scroll_right(&mut self) {
        let max_scroll = self.compute_total_width().saturating_sub(self.area_width as usize);
        if self.scroll_x < max_scroll {
            self.scroll_x = self.scroll_x.saturating_add(SCROLL_SIZE).min(max_scroll);
        }
    }

    fn scroll_left(&mut self) {
        if self.compute_total_width() > self.area_width as usize {
            self.scroll_x = self.scroll_x.saturating_sub(SCROLL_SIZE);
        }
    }

    fn send_task_to_next(&mut self) {
        if self.focused_column == self.columns.len() -1 {
            return;
        }

        self.send_task_to_column(self.focused_task, self.focused_column, self.focused_column +1);
        self.focused_column = self.focused_column.saturating_add(1);
        self.focused_task = self.columns[self.focused_column].tasks.len() -1;
    }
    fn send_task_to_prev(&mut self) {
        if self.focused_column == 0 {
            return;
        }

        self.send_task_to_column(self.focused_task, self.focused_column, self.focused_column -1);
        self.focused_column = self.focused_column.saturating_sub(1);
        self.focused_task = self.columns[self.focused_column].tasks.len() -1;
    }

    fn send_task_to_column(&mut self, task_index: usize, source_column_index: usize, target_column_index: usize) {
        let task = self.columns[source_column_index].tasks[task_index].clone();
        self.columns[source_column_index].tasks.remove(task_index);
        self.columns[target_column_index].tasks.push(task);
    }

    fn left_space(&self, column_index: usize) -> usize {
        let mut space: usize = 0;
        for i in 0..column_index {
            let col = &self.columns[i];
            space = space.saturating_add(col.width);
            space = space.saturating_add(COLUMNS_GAP as usize);
        }

        space
    }

    fn scroll_to_focused_column(&mut self) {
        let column = &self.columns[self.focused_column];
        let column_l = self.left_space(self.focused_column);
        let column_r = column_l + column.width;

        if column_r > self.scroll_x + self.area_width as usize {
            self.scroll_x = column_r.saturating_sub(self.area_width as usize);
        } else if column_l < self.scroll_x {
            self.scroll_x = column_l;
        }
    }
}