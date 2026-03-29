use color_eyre::eyre::Result;
use crossterm::event;
use crossterm::event::{KeyCode, KeyEventKind};
use ratatui::{DefaultTerminal, Frame};
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};

use crate::models::column::{Column, CreatingColumnPopup};
use crate::models::task::{CreatingTaskPopup, Task, TaskPriority};

const SCROLL_SIZE: usize = 5;
const COLUMNS_GAP: u16 = 4;
enum InputMode {
    Normal,
    Editing,
    Visual,
}

enum InputType {
    CreatingColumn,
    CreatingTaskTitle,
    CreatingTaskDescription,
    CreatingTaskPriority,
}

pub struct App {
    input_mode: InputMode,
    columns: Vec<Column>,
    scroll_x: usize,
    scroll_x_state: ScrollbarState,
    focused_column: usize,
    area_width: u16,

    creating_column_popup: Option<CreatingColumnPopup>,
    creating_task_popup: Option<CreatingTaskPopup>,
    active_input: Option<InputType>
}

impl App {
    pub(crate) fn new() -> Self {
        let mut columns = Vec::new();
        columns.push(Column::new("TODO".to_string(), 50));
        columns.push(Column::new("Doing".to_string(), 50));
        columns.push(Column::new("Waiting Review".to_string(), 50));
        columns.push(Column::new("Done".to_string(), 50));

        Self {
            input_mode: InputMode::Normal,
            columns,
            scroll_x: 0,
            scroll_x_state: ScrollbarState::new(5),
            area_width: 0,
            focused_column: 0,
            creating_column_popup: None,
            creating_task_popup: None,
            active_input: None,
        }
    }

    pub(crate) fn run(mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        loop {
            terminal.draw(|frame| self.render(frame))?;

            if let Some(key) = event::read()?.as_key_press_event() {
                match self.input_mode {
                    InputMode::Normal => match key.code {
                        KeyCode::Char('i') => self.input_mode = InputMode::Editing,
                        KeyCode::Char('c') => {
                            self.input_mode = InputMode::Editing;
                            self.toggle_column_creation_popup();
                        }
                        KeyCode::Char('t') => {
                            self.input_mode = InputMode::Editing;
                            self.toggle_task_creation_popup();
                        }
                        KeyCode::Char('d') => {
                            self.delete_column(self.focused_column)
                        }
                        KeyCode::Left => {
                            self.scroll_left()
                        }
                        KeyCode::Right => {
                            self.scroll_right()
                        }
                        KeyCode::Tab => {
                            self.focused_column = self.focused_column.saturating_add(1) % self.columns.len();
                            self.scroll_to_focused_column();
                        }
                        KeyCode::BackTab => {
                            self.focused_column = self.focused_column.checked_sub(1).unwrap_or(self.columns.len() - 1)
                        }
                        KeyCode::Char('q') => return Ok(()),
                        _ => {},
                    }
                    InputMode::Editing if key.kind == KeyEventKind::Press => match key.code {
                        KeyCode::Esc => {
                            match self.active_input {
                                Some(InputType::CreatingColumn) => {
                                    self.toggle_column_creation_popup()
                                }
                                Some(InputType::CreatingTaskTitle) => {}
                                Some(InputType::CreatingTaskDescription) => {}
                                Some(InputType::CreatingTaskPriority) => {}
                                None => {}
                            }
                            self.active_input = None;
                            self.input_mode = InputMode::Normal;
                        }
                        KeyCode::Char(c) => {
                            match self.active_input {
                                Some(InputType::CreatingColumn) => {
                                    if let Some(popup) = &mut self.creating_column_popup {
                                        popup.input.push(c);
                                    }
                                }
                                Some(InputType::CreatingTaskTitle) => {
                                    if let Some(popup) = &mut self.creating_task_popup {
                                        popup.title.push(c);
                                    }}
                                Some(InputType::CreatingTaskDescription) => {
                                    if let Some(popup) = &mut self.creating_task_popup {
                                        popup.description.push(c);
                                    }}
                                Some(InputType::CreatingTaskPriority) => {}
                                None => {}
                            }
                        }
                        KeyCode::Backspace => {
                            match self.active_input {
                                Some(InputType::CreatingColumn) => {
                                    if let Some(popup) = &mut self.creating_column_popup {
                                        popup.input.pop();
                                    }
                                }
                                Some(InputType::CreatingTaskTitle) => {
                                    if let Some(popup) = &mut self.creating_task_popup {
                                        popup.title.pop();
                                    }}
                                Some(InputType::CreatingTaskDescription) => {
                                    if let Some(popup) = &mut self.creating_task_popup {
                                        popup.description.pop();
                                    }}
                                Some(InputType::CreatingTaskPriority) => {}
                                None => {}
                            }
                        }
                        KeyCode::Enter => {
                            match self.active_input {
                                Some(InputType::CreatingColumn) => {
                                    if let Some(popup) = &self.creating_column_popup {
                                        self.columns.push(Column::new(popup.input.clone(), 50));
                                    }
                                    self.creating_column_popup = None;
                                    self.active_input = None;
                                    self.input_mode = InputMode::Normal;
                                }
                                Some(InputType::CreatingTaskTitle) => {
                                    if self.safe_create_task() { continue; }
                                }
                                Some(InputType::CreatingTaskDescription) => {
                                    if self.safe_create_task() { continue; }
                                }
                                Some(InputType::CreatingTaskPriority) => {
                                    if self.safe_create_task() { continue; }
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
                        KeyCode::Right => {
                            if matches!(self.active_input, Some(InputType::CreatingTaskPriority)) {
                                if let Some(popup) = &mut self.creating_task_popup {
                                    popup.priority = match popup.priority {
                                        TaskPriority::LOW => TaskPriority::MEDIUM,
                                        TaskPriority::MEDIUM => TaskPriority::HIGH,
                                        TaskPriority::HIGH => TaskPriority::LOW,
                                    };
                                }
                            }
                        }
                        KeyCode::Left => {
                            if matches!(self.active_input, Some(InputType::CreatingTaskPriority)) {
                                if let Some(popup) = &mut self.creating_task_popup {
                                    popup.priority = match popup.priority {
                                        TaskPriority::LOW => TaskPriority::HIGH,
                                        TaskPriority::MEDIUM => TaskPriority::LOW,
                                        TaskPriority::HIGH => TaskPriority::MEDIUM,
                                    };
                                }
                            }
                        }
                        _ => {}
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn safe_create_task(&mut self) -> bool {
        if let Some(popup) = &self.creating_task_popup {
            if popup.title.is_empty() || popup.description.is_empty() {
                // TODO : DISPLAY ERROR WHEN EMPTY ON CREATION
                return true;
            }

            let task: Task = Task::new(
                popup.title.clone(),
                popup.description.clone(),
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
                        .border_style(Style::default().fg(priority_color));
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
    }

    fn render_column_creation_popup(&mut self, frame: &mut Frame, area: Rect) {
        if let Some(popup) = &self.creating_column_popup {
            let popup_block = Block::default()
                .title("New column name")
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::DarkGray));

            let area = area.centered(Constraint::Percentage(60), Constraint::Length(3));
            let inner_area = popup_block.inner(area);

            frame.render_widget(Clear, area);
            frame.render_widget(popup_block, area);
            frame.render_widget(Paragraph::new(popup.input.as_str()), inner_area);
        }
    }

    fn render_task_creation_popup(&mut self, frame: &mut Frame, area: Rect) {
        if let Some(t_popup) = &self.creating_task_popup {
            let popup_block = Block::default()
                .title("Create a new task") // IDEA : MAYBE ADD COLUMN NAME
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::DarkGray));

            let area = area.centered(Constraint::Percentage(60), Constraint::Min(11));
            let inner_area = popup_block.inner(area);

            let layout = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(3),
                Constraint::Length(3),
            ]);
            let [title_area, description_area, priority_area] = inner_area.layout(&layout);

            frame.render_widget(Clear, area);
            frame.render_widget(popup_block, area);

            let title_block = Block::default()
                .title("Title")
                .borders(Borders::ALL)
                .border_style(if matches!(self.active_input, Some(InputType::CreatingTaskTitle)) {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                });
            frame.render_widget(Paragraph::new(t_popup.title.as_str()).block(title_block), title_area);

            let desc_block = Block::default()
                .title("Description")
                .borders(Borders::ALL)
                .border_style(if matches!(self.active_input, Some(InputType::CreatingTaskDescription)) {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                });
            frame.render_widget(Paragraph::new(t_popup.description.as_str()).block(desc_block), description_area);

            // Priority
            let priority_block = Block::default()
                .title("Priority")
                .borders(Borders::ALL);
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
        match self.creating_column_popup {
            Some(_) => {
                self.creating_column_popup = None;
                self.active_input = None;
            }
            None => {
                self.creating_column_popup = Some(CreatingColumnPopup::new());
                self.active_input = Some(InputType::CreatingColumn);
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