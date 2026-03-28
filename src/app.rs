use color_eyre::eyre::Result;
use crossterm::event;
use crossterm::event::{KeyCode, KeyEventKind};
use ratatui::{DefaultTerminal, Frame};
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};

use crate::models::column::Column;

enum InputMode {
    Normal,
    Editing,
    Visual,
}

pub struct App {
    input_mode: InputMode,
    columns: Vec<Column>,
    scroll_x: usize,
    scroll_x_state: ScrollbarState,
    focused_column: usize,
    area_width: u16,
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
            scroll_x_state: ScrollbarState::new(100),
            area_width: 0,
            focused_column: 0,
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
                            self.create_column()
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
                            self.focused_column = self.focused_column.saturating_add(1) % self.columns.len()
                        }
                        KeyCode::BackTab => {
                            self.focused_column = self.focused_column.checked_sub(1).unwrap_or(self.columns.len() - 1)
                        }
                        KeyCode::Char('q') => return Ok(()),
                        _ => {},
                    }
                    InputMode::Editing if key.kind == KeyEventKind::Press => match key.code {
                        KeyCode::Esc => self.input_mode = InputMode::Normal,
                        _ => {}
                    }
                    _ => {}
                }
            }
        }
        Ok(())
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
        const column_gap: u16 = 4;


        let content_width = self.compute_total_width();
        self.scroll_x_state = self.scroll_x_state.content_length(content_width).position(self.scroll_x);
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

            let column_left = i as u16 * (ongoing_column.width.clone() as u16 + column_gap);
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

            if i == self.focused_column {
                frame.render_widget(Block::bordered().title(ongoing_column.name.clone()).border_style(Style::new().bold()), column);
            } else {
                frame.render_widget(Block::bordered().title(ongoing_column.name.clone()), column);
            }
        }

        if needs_scroll {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::HorizontalBottom);
            frame.render_stateful_widget(scrollbar, area.inner(Margin {
                horizontal: 1,
                vertical: 0,
            }), &mut self.scroll_x_state);
        }
    }

    fn create_column(&mut self) {
        self.columns.push(Column::new("New column".to_string(), 35));
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
        return self.columns.len() * 30;
    }

    fn scroll_right(&mut self) {
        if self.compute_total_width() > self.area_width as usize {
            self.scroll_x = self.scroll_x.saturating_add(3);
        }
    }

    fn scroll_left(&mut self) {
        if self.compute_total_width() > self.area_width as usize {
            self.scroll_x = self.scroll_x.saturating_sub(3);
        }
    }
}