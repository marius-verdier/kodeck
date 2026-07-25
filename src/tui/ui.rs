use std::path::Path;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};

use super::state::StatusLevel;

pub(super) const MIN_TERMINAL_WIDTH: u16 = 50;
pub(super) const MIN_TERMINAL_HEIGHT: u16 = 14;
pub(super) const MIN_COLUMN_WIDTH: u16 = 28;
pub(super) const MAX_COLUMN_WIDTH: u16 = 42;
pub(super) const MAX_VISIBLE_COLUMNS: usize = 4;
pub(super) const COLUMN_GAP: u16 = 1;
pub(super) const DETAIL_BREAKPOINT: u16 = 120;

#[derive(Debug, Clone, Copy)]
pub(super) struct UiTheme {
    pub focus: Color,
    pub error: Color,
    pub warning: Color,
    pub secondary: Color,
}

impl Default for UiTheme {
    fn default() -> Self {
        Self {
            focus: Color::Cyan,
            error: Color::Red,
            warning: Color::Yellow,
            secondary: Color::DarkGray,
        }
    }
}

impl UiTheme {
    pub fn focus(self) -> Style {
        Style::default().fg(self.focus).add_modifier(Modifier::BOLD)
    }

    pub fn status(self, level: StatusLevel) -> Style {
        match level {
            StatusLevel::Info => Style::default().fg(self.focus),
            StatusLevel::Warn => Style::default().fg(self.warning),
            StatusLevel::Error => Style::default().fg(self.error),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DetailPlacement {
    Closed,
    Panel,
    Overlay,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BoardLayout {
    pub too_small: bool,
    pub header: Rect,
    pub board: Rect,
    pub detail: Option<Rect>,
    pub status: Rect,
    pub footer: Rect,
    pub column_width: u16,
    pub visible_columns: usize,
    pub detail_placement: DetailPlacement,
}

impl BoardLayout {
    pub fn compute(area: Rect, column_count: usize, detail_open: bool) -> Self {
        if area.width < MIN_TERMINAL_WIDTH || area.height < MIN_TERMINAL_HEIGHT {
            return Self {
                too_small: true,
                header: area,
                board: area,
                detail: None,
                status: area,
                footer: area,
                column_width: 0,
                visible_columns: 0,
                detail_placement: DetailPlacement::Closed,
            };
        }

        let [header, content, status, footer] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);

        let placement = if !detail_open {
            DetailPlacement::Closed
        } else if area.width >= DETAIL_BREAKPOINT {
            DetailPlacement::Panel
        } else {
            DetailPlacement::Overlay
        };
        let (board, detail) = if placement == DetailPlacement::Panel {
            let detail_width = (area.width / 3).clamp(36, 48);
            let [board, _, detail] = Layout::horizontal([
                Constraint::Fill(1),
                Constraint::Length(1),
                Constraint::Length(detail_width),
            ])
            .areas(content);
            (board, Some(detail))
        } else {
            (content, None)
        };

        let possible =
            usize::from((board.width + COLUMN_GAP) / (MIN_COLUMN_WIDTH + COLUMN_GAP)).max(1);
        let visible_columns = column_count.clamp(1, MAX_VISIBLE_COLUMNS).min(possible);
        let gaps = COLUMN_GAP.saturating_mul(visible_columns.saturating_sub(1) as u16);
        let column_width = board
            .width
            .saturating_sub(gaps)
            .checked_div(visible_columns as u16)
            .unwrap_or(board.width)
            .clamp(MIN_COLUMN_WIDTH.min(board.width), MAX_COLUMN_WIDTH);

        Self {
            too_small: false,
            header,
            board,
            detail,
            status,
            footer,
            column_width,
            visible_columns,
            detail_placement: placement,
        }
    }

    pub fn column_areas(&self) -> Vec<Rect> {
        (0..self.visible_columns)
            .map(|index| Rect {
                x: self
                    .board
                    .x
                    .saturating_add(index as u16 * (self.column_width + COLUMN_GAP)),
                y: self.board.y,
                width: self.column_width,
                height: self.board.height,
            })
            .filter(|area| area.right() <= self.board.right())
            .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ViewportState {
    pub first_column: usize,
    pub card_offsets: Vec<usize>,
}

impl ViewportState {
    pub fn ensure_columns(&mut self, count: usize) {
        self.card_offsets.resize(count, 0);
        self.first_column = self.first_column.min(count.saturating_sub(1));
    }

    pub fn reveal_column(&mut self, focused: usize, visible: usize, total: usize) {
        if visible == 0 || total == 0 {
            self.first_column = 0;
        } else if focused < self.first_column {
            self.first_column = focused;
        } else if focused >= self.first_column + visible {
            self.first_column = focused + 1 - visible;
        }
        self.first_column = self
            .first_column
            .min(total.saturating_sub(visible.min(total)));
    }

    pub fn reveal_card(&mut self, column: usize, focused: usize, capacity: usize) {
        let Some(offset) = self.card_offsets.get_mut(column) else {
            return;
        };
        if capacity == 0 || focused < *offset {
            *offset = focused;
        } else if focused >= *offset + capacity {
            *offset = focused + 1 - capacity;
        }
    }
}

pub(super) fn modal_area(area: Rect, preferred_width: u16, preferred_height: u16) -> Rect {
    let width = preferred_width.min(area.width.saturating_sub(4)).max(1);
    let height = preferred_height.min(area.height.saturating_sub(2)).max(1);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

pub(super) fn truncate(value: &str, width: usize) -> String {
    let count = value.chars().count();
    if count <= width {
        return value.to_owned();
    }
    if width <= 1 {
        return "…".chars().take(width).collect();
    }
    value.chars().take(width - 1).chain(['…']).collect()
}

pub(super) fn abbreviated_path(path: &Path, width: usize) -> String {
    let display = path.display().to_string();
    if display.chars().count() <= width {
        return display;
    }
    let components: Vec<_> = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect();
    for keep in 1..=components.len() {
        let candidate = format!("…/{}", components[components.len() - keep..].join("/"));
        if candidate.chars().count() > width {
            if keep == 1 {
                return truncate(&candidate, width);
            }
            return format!("…/{}", components[components.len() - keep + 1..].join("/"));
        }
    }
    truncate(&display, width)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responsive_layout_uses_only_complete_columns() {
        for (width, expected) in [(40, 0), (60, 2), (80, 2), (120, 4), (160, 4)] {
            let layout = BoardLayout::compute(Rect::new(0, 0, width, 30), 8, false);
            if width < MIN_TERMINAL_WIDTH {
                assert!(layout.too_small);
                continue;
            }
            assert_eq!(layout.visible_columns, expected);
            assert!((MIN_COLUMN_WIDTH..=MAX_COLUMN_WIDTH).contains(&layout.column_width));
            assert!(
                layout
                    .column_areas()
                    .iter()
                    .all(|area| area.right() <= layout.board.right())
            );
        }
    }

    #[test]
    fn detail_switches_between_overlay_and_panel() {
        let compact = BoardLayout::compute(Rect::new(0, 0, 80, 24), 5, true);
        assert_eq!(compact.detail_placement, DetailPlacement::Overlay);
        assert!(compact.detail.is_none());

        let wide = BoardLayout::compute(Rect::new(0, 0, 120, 24), 5, true);
        assert_eq!(wide.detail_placement, DetailPlacement::Panel);
        assert!((36..=48).contains(&wide.detail.unwrap().width));
    }

    #[test]
    fn viewport_reveals_columns_and_whole_cards() {
        let mut viewport = ViewportState::default();
        viewport.ensure_columns(8);
        viewport.reveal_column(6, 3, 8);
        assert_eq!(viewport.first_column, 4);
        viewport.reveal_column(1, 3, 8);
        assert_eq!(viewport.first_column, 1);

        viewport.reveal_card(1, 9, 4);
        assert_eq!(viewport.card_offsets[1], 6);
        viewport.reveal_card(1, 2, 4);
        assert_eq!(viewport.card_offsets[1], 2);
    }

    #[test]
    fn truncation_marks_long_text() {
        assert_eq!(truncate("A very long title", 8), "A very …");
        assert_eq!(truncate("short", 8), "short");
    }
}
