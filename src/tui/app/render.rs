use std::collections::HashMap;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap,
};

use crate::domain::{AnnotationDisposition, TaskPriority};
use crate::tui::keymap::{self, KeyContext};
use crate::tui::state::{Confirmation, InputType, StatusLevel, StatusMessage, TaskFormMode};
use crate::tui::ui::{BoardLayout, DetailPlacement, abbreviated_path, modal_area, truncate};

use super::App;

impl App<'_> {
    pub(super) fn render(&mut self, frame: &mut Frame) {
        let layout = BoardLayout::compute(frame.area(), self.columns.len(), self.detail.is_some());
        if layout.too_small {
            frame.render_widget(
                Paragraph::new("Terminal too small")
                    .alignment(Alignment::Center)
                    .style(self.theme.focus()),
                frame.area(),
            );
            return;
        }

        self.viewport.ensure_columns(self.columns.len());
        self.viewport.reveal_column(
            self.focused_column,
            layout.visible_columns,
            self.columns.len(),
        );
        let card_capacity = usize::from(layout.board.height.saturating_sub(2) / 2).max(1);
        self.viewport
            .reveal_card(self.focused_column, self.focused_task, card_capacity);

        self.render_header(frame, &layout);
        self.render_board(frame, &layout);
        if let Some(detail_area) = layout.detail {
            self.render_detail(frame, detail_area, false);
        }
        self.render_status(frame, &layout);
        self.render_footer(frame, &layout);

        if layout.detail_placement == DetailPlacement::Overlay && self.detail.is_some() {
            let area = modal_area(
                layout.board,
                layout.board.width.saturating_sub(4),
                layout.board.height,
            );
            self.render_detail(frame, area, true);
        }
        self.render_confirmation(frame, layout.board);
        self.render_move_picker(frame, layout.board);
        self.render_goto_menu(frame, layout.board);
        self.render_help(frame, layout.board);
        self.render_annotation_review(frame, layout.board);
        self.render_column_form(frame, layout.board);
        self.render_task_form(frame, layout.board);
    }

    pub(super) fn render_header(&self, frame: &mut Frame, layout: &BoardLayout) {
        let card_count: usize = self.columns.iter().map(|column| column.tasks.len()).sum();
        let title = Line::from(vec![
            Span::styled("KODECK", self.theme.focus()),
            Span::raw("  "),
            Span::styled(
                self.workspace_config.name.as_str(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]);
        let summary = Line::from(vec![
            Span::styled(
                format!("{} columns  {} cards", self.columns.len(), card_count),
                Style::default().fg(self.theme.secondary),
            ),
            Span::raw("  "),
            Span::styled(
                format!(
                    "{} repositories  {} annotations",
                    self.workspace_config.repositories.len(),
                    self.findings_count
                ),
                Style::default().fg(self.theme.secondary),
            ),
        ]);
        frame.render_widget(Paragraph::new(vec![title, summary]), layout.header);
    }

    pub(super) fn render_board(&mut self, frame: &mut Frame, layout: &BoardLayout) {
        let goto_labels: HashMap<_, _> = self
            .goto_labels
            .as_ref()
            .map(|state| {
                state
                    .targets
                    .iter()
                    .map(|target| {
                        (
                            target.card_id,
                            (target.label.clone(), target.label.starts_with(&state.input)),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut visible_cards = Vec::new();

        for (slot, column_area) in layout.column_areas().into_iter().enumerate() {
            let column_index = self.viewport.first_column + slot;
            let Some(column) = self.columns.get(column_index) else {
                continue;
            };
            let focused = column_index == self.focused_column;
            let title = format!(" {} ({}) ", column.name, column.tasks.len());
            let block = Block::default()
                .title(truncate(
                    &title,
                    column_area.width.saturating_sub(2) as usize,
                ))
                .borders(Borders::ALL)
                .border_style(if focused {
                    self.theme.focus()
                } else {
                    Style::default().fg(self.theme.secondary)
                });
            let inner = block.inner(column_area);
            frame.render_widget(block, column_area);

            if column.tasks.is_empty() {
                frame.render_widget(
                    Paragraph::new("No cards")
                        .style(Style::default().fg(self.theme.secondary))
                        .alignment(Alignment::Center),
                    inner,
                );
                continue;
            }

            let offset = self
                .viewport
                .card_offsets
                .get(column_index)
                .copied()
                .unwrap_or(0)
                .min(column.tasks.len().saturating_sub(1));
            let capacity = usize::from(inner.height / 2);
            for (visible_index, (task_index, task)) in column
                .tasks
                .iter()
                .enumerate()
                .skip(offset)
                .take(capacity)
                .enumerate()
            {
                visible_cards.push(task.id);
                let row = Rect {
                    x: inner.x,
                    y: inner.y + visible_index as u16 * 2,
                    width: inner.width,
                    height: 2,
                };
                let is_focused = focused && task_index == self.focused_task;
                let is_selected = self.selected_cards.contains(&task.id);
                let goto = goto_labels.get(&task.id);
                let marker = if is_focused {
                    ">"
                } else if is_selected {
                    "*"
                } else {
                    " "
                };
                let label = goto
                    .map(|(label, _)| format!("[{label}] "))
                    .unwrap_or_default();
                let title_width = row.width.saturating_sub(2) as usize;
                let title = truncate(&format!("{marker} {label}{}", task.title), title_width);
                let mut title_style = if is_focused || is_selected {
                    self.theme.focus()
                } else {
                    Style::default()
                };
                if goto.is_some_and(|(_, matches)| !matches) {
                    title_style = Style::default().fg(self.theme.secondary);
                }
                let priority_style = match task.priority {
                    TaskPriority::HIGH => Style::default().fg(self.theme.error),
                    TaskPriority::MEDIUM => Style::default().fg(self.theme.warning),
                    TaskPriority::LOW => Style::default().fg(self.theme.secondary),
                };
                frame.render_widget(
                    Paragraph::new(vec![
                        Line::styled(title, title_style),
                        Line::from(vec![
                            Span::raw("  "),
                            Span::styled(task.priority.value(), priority_style),
                            Span::styled("  LOCAL", Style::default().fg(self.theme.secondary)),
                        ]),
                    ]),
                    row,
                );
            }
        }
        self.visible_cards = visible_cards;
    }

    pub(super) fn render_status(&self, frame: &mut Frame, layout: &BoardLayout) {
        let explicit = self
            .last_error
            .as_ref()
            .map(|error| StatusMessage::error(error.clone()))
            .or_else(|| self.status_message.clone());
        if let Some(message) = explicit {
            frame.render_widget(
                Paragraph::new(format!(
                    "{}  {}",
                    match message.level {
                        StatusLevel::Info => "INFO",
                        StatusLevel::Warn => "WARN",
                        StatusLevel::Error => "ERROR",
                    },
                    message.text
                ))
                .style(self.theme.status(message.level)),
                layout.status,
            );
            return;
        }

        let path = abbreviated_path(&self.workspace_root, (layout.status.width / 2) as usize);
        let column = self.columns.get(self.focused_column);
        let position = column
            .and_then(|column| {
                (!column.tasks.is_empty()).then(|| {
                    format!(
                        "card {}/{}",
                        self.focused_task.saturating_add(1),
                        column.tasks.len()
                    )
                })
            })
            .unwrap_or_else(|| "No cards".to_owned());
        let selected = if self.selected_cards.is_empty() {
            String::new()
        } else {
            format!("  {} selected", self.selected_cards.len())
        };
        frame.render_widget(
            Paragraph::new(format!(
                "{path}  |  column {}/{} {}  |  {position}{selected}",
                self.focused_column.saturating_add(1),
                self.columns.len(),
                column.map(|column| column.name.as_str()).unwrap_or("")
            ))
            .style(Style::default().fg(self.theme.secondary)),
            layout.status,
        );
    }

    pub(super) fn render_footer(&self, frame: &mut Frame, layout: &BoardLayout) {
        let hints = if self.goto_labels.is_some() {
            "Type a two-letter card label  Esc cancel".to_owned()
        } else if self.key_context() == KeyContext::Board && layout.footer.width <= 100 {
            "n new  e edit  H/L cards  [/] cols  m move  Space select  g goto  ? help  q quit"
                .to_owned()
        } else {
            keymap::footer_hints(self.key_context())
        };
        frame.render_widget(
            Paragraph::new(truncate(&hints, layout.footer.width as usize))
                .style(Style::default().fg(self.theme.secondary)),
            layout.footer,
        );
    }

    pub(super) fn modal_block(&self, title: impl Into<Line<'static>>) -> Block<'static> {
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(self.theme.focus())
            .padding(Padding::horizontal(1))
    }

    pub(super) fn render_detail(&self, frame: &mut Frame, area: Rect, clear: bool) {
        let Some(detail) = self.detail else {
            return;
        };
        let Some((column_index, task_index)) = self.locate_card(detail.card_id) else {
            return;
        };
        let task = &self.columns[column_index].tasks[task_index];
        if clear {
            frame.render_widget(Clear, area);
        }
        let block = self.modal_block(" Details ");
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let priority_style = match task.priority {
            TaskPriority::HIGH => Style::default().fg(self.theme.error),
            TaskPriority::MEDIUM => Style::default().fg(self.theme.warning),
            TaskPriority::LOW => Style::default().fg(self.theme.secondary),
        };
        let [
            title_area,
            metadata_area,
            location_area,
            description_title,
            description_area,
            actions,
        ] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(inner);
        frame.render_widget(
            Paragraph::new(task.title.as_str())
                .style(Style::default().add_modifier(Modifier::BOLD)),
            title_area,
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(task.priority.value(), priority_style),
                Span::styled("  LOCAL", Style::default().fg(self.theme.secondary)),
            ])),
            metadata_area,
        );
        frame.render_widget(
            Paragraph::new(format!(
                "{}  |  card {}/{}",
                self.columns[column_index].name,
                task_index + 1,
                self.columns[column_index].tasks.len()
            ))
            .style(Style::default().fg(self.theme.secondary)),
            location_area,
        );
        frame.render_widget(
            Paragraph::new("Description").style(self.theme.focus()),
            description_title,
        );
        let sources = self
            .annotations
            .iter()
            .filter(|annotation| {
                matches!(
                    annotation.disposition,
                    AnnotationDisposition::Linked { card_id } if card_id == task.id
                )
            })
            .map(|annotation| {
                format!(
                    "[{}] {}:{}:{}  {}",
                    if annotation.present {
                        "present"
                    } else {
                        "missing"
                    },
                    annotation.repository_id,
                    annotation.path,
                    annotation.line,
                    annotation.tag
                )
            })
            .collect::<Vec<_>>();
        let description = if sources.is_empty() {
            task.description.clone()
        } else {
            format!("Sources\n{}\n\n{}", sources.join("\n"), task.description)
        };
        frame.render_widget(
            Paragraph::new(description)
                .scroll((detail.scroll, 0))
                .wrap(Wrap { trim: false }),
            description_area,
        );
        frame.render_widget(
            Paragraph::new("e edit  d done  x archive")
                .style(Style::default().fg(self.theme.secondary)),
            actions,
        );
    }

    pub(super) fn render_confirmation(&self, frame: &mut Frame, area: Rect) {
        let Some(confirmation) = &self.confirmation else {
            return;
        };
        let (title, detail) = match &confirmation.action {
            Confirmation::Archive { card_ids } => (
                format!(" Archive {} card(s)? ", card_ids.len()),
                card_ids
                    .iter()
                    .filter_map(|card_id| {
                        self.locate_card(*card_id)
                            .map(|(column, task)| self.columns[column].tasks[task].title.clone())
                    })
                    .take(3)
                    .collect::<Vec<_>>()
                    .join("  |  "),
            ),
            Confirmation::DeleteColumn { column_index } => (
                format!(" Delete column '{}'? ", self.columns[*column_index].name),
                "The shared workspace configuration will be updated.".to_owned(),
            ),
        };
        let popup = modal_area(area, 64, 7);
        frame.render_widget(Clear, popup);
        let block = self.modal_block(title);
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        let [detail_area, choice_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(inner);
        frame.render_widget(
            Paragraph::new(detail)
                .wrap(Wrap { trim: true })
                .alignment(Alignment::Center),
            detail_area,
        );
        let cancel_style = if !confirmation.confirm_selected {
            self.theme.focus().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let confirm_style = if confirmation.confirm_selected {
            self.theme.focus().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" Cancel ", cancel_style),
                Span::styled(" / ", Style::default().fg(self.theme.secondary)),
                Span::styled(" Confirm ", confirm_style),
            ]))
            .alignment(Alignment::Center),
            choice_area,
        );
    }

    pub(super) fn render_move_picker(&self, frame: &mut Frame, area: Rect) {
        let Some(picker) = &self.move_picker else {
            return;
        };
        let popup = modal_area(area, 48, (self.columns.len() as u16 + 2).min(16));
        let items = self
            .columns
            .iter()
            .map(|column| ListItem::new(format!("{}  ({})", column.name, column.tasks.len())))
            .collect::<Vec<_>>();
        let mut state = ListState::default().with_selected(Some(picker.selected_column));
        frame.render_widget(Clear, popup);
        frame.render_stateful_widget(
            List::new(items)
                .block(self.modal_block(" Move cards to "))
                .highlight_style(self.theme.focus())
                .highlight_symbol("> "),
            popup,
            &mut state,
        );
    }

    pub(super) fn render_goto_menu(&self, frame: &mut Frame, area: Rect) {
        if !self.goto_pending {
            return;
        }
        let popup = modal_area(area, 46, 7);
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Paragraph::new(
                "gw  visible card\ngg  first card    ge  last card\ngh  first column  gl  last column",
            )
            .block(self.modal_block(" Goto "))
            .wrap(Wrap { trim: false }),
            popup,
        );
    }

    pub(super) fn render_help(&self, frame: &mut Frame, area: Rect) {
        if !self.help_open {
            return;
        }
        let popup = modal_area(
            area,
            area.width.saturating_mul(3) / 4,
            area.height.saturating_sub(2),
        );
        let lines = keymap::help_lines();
        let max_scroll = lines
            .len()
            .saturating_sub(popup.height.saturating_sub(2) as usize);
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Paragraph::new(lines.join("\n"))
                .scroll((self.help_scroll.min(max_scroll) as u16, 0))
                .block(self.modal_block(" Keyboard shortcuts "))
                .wrap(Wrap { trim: false }),
            popup,
        );
    }

    pub(super) fn render_annotation_review(&self, frame: &mut Frame, area: Rect) {
        let Some(review) = &self.annotation_review else {
            return;
        };
        let ids = self.visible_annotation_ids();
        let popup = modal_area(
            area,
            area.width.saturating_sub(4),
            area.height.saturating_sub(2),
        );
        frame.render_widget(Clear, popup);
        let sync_status = if self.annotation_sync.is_some() {
            " · syncing…"
        } else {
            ""
        };
        let title = format!(
            " Annotations · {} · {} selected{sync_status} ",
            review.filter.label(),
            review.selected.len()
        );
        let block = self.modal_block(title);
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        let [list_area, actions] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(inner);
        if ids.is_empty() {
            frame.render_widget(
                Paragraph::new("No annotations in this filter")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(self.theme.secondary)),
                list_area,
            );
        } else {
            let mut previous_group: Option<(String, String)> = None;
            let items = ids
                .iter()
                .filter_map(|id| {
                    self.annotations
                        .iter()
                        .find(|annotation| annotation.id == *id)
                })
                .map(|annotation| {
                    let group = (
                        annotation.repository_id.as_str().to_owned(),
                        annotation.path.clone(),
                    );
                    let group_line = if previous_group.as_ref() == Some(&group) {
                        Line::raw("")
                    } else {
                        previous_group = Some(group.clone());
                        Line::styled(format!("{} / {}", group.0, group.1), self.theme.focus())
                    };
                    let badge = if !annotation.present {
                        "MISSING"
                    } else {
                        match annotation.disposition {
                            AnnotationDisposition::Unassigned => "NEW",
                            AnnotationDisposition::Ignored => "IGNORED",
                            AnnotationDisposition::Linked { .. } => "LINKED",
                        }
                    };
                    let marker = if review.selected.contains(&annotation.id) {
                        "*"
                    } else {
                        " "
                    };
                    ListItem::new(vec![
                        group_line,
                        Line::from(vec![
                            Span::styled(
                                format!("{marker} [{badge:<7}] "),
                                Style::default().fg(self.theme.secondary),
                            ),
                            Span::styled(
                                format!("{}:{} ", annotation.tag, annotation.line),
                                Style::default().add_modifier(Modifier::BOLD),
                            ),
                            Span::raw(annotation.message.clone()),
                        ]),
                    ])
                })
                .collect::<Vec<_>>();
            let mut state = ListState::default()
                .with_selected(Some(review.focused.min(items.len().saturating_sub(1))));
            frame.render_stateful_widget(
                List::new(items)
                    .highlight_style(self.theme.focus())
                    .highlight_symbol("> "),
                list_area,
                &mut state,
            );
        }
        frame.render_widget(
            Paragraph::new(
                "j/k navigate  Space select  c create card  i ignore  f filter  Ctrl+R rescan  Esc close",
            )
            .style(Style::default().fg(self.theme.secondary)),
            actions,
        );
    }

    pub(super) fn render_column_form(&self, frame: &mut Frame, area: Rect) {
        let Some(form) = &self.create_column_popup else {
            return;
        };
        let popup = modal_area(area, 52, 7);
        frame.render_widget(Clear, popup);
        let block = self.modal_block(" New column ");
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        let [input_area, error_area, action_area] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(inner);
        let input_block = Block::bordered()
            .title(" Name ")
            .border_style(self.theme.focus());
        let scroll = form
            .input
            .visual_scroll(input_area.width.saturating_sub(3) as usize);
        frame.render_widget(
            Paragraph::new(form.input.value())
                .scroll((0, scroll as u16))
                .block(input_block),
            input_area,
        );
        if let Some(error) = &form.error {
            frame.render_widget(
                Paragraph::new(error.as_str()).style(Style::default().fg(self.theme.error)),
                error_area,
            );
        }
        frame.render_widget(
            Paragraph::new("Ctrl+S create  Esc cancel")
                .style(Style::default().fg(self.theme.secondary)),
            action_area,
        );
        let cursor = form.input.visual_cursor().max(scroll) - scroll + 1;
        frame.set_cursor_position((input_area.x + cursor as u16, input_area.y + 1));
    }

    pub(super) fn render_task_form(&mut self, frame: &mut Frame, area: Rect) {
        let Some(form) = &mut self.task_form else {
            return;
        };
        let popup = modal_area(
            area,
            area.width.saturating_sub(8).min(80),
            area.height.saturating_sub(2).min(21),
        );
        frame.render_widget(Clear, popup);
        let title = match &form.mode {
            TaskFormMode::Create => " New card ",
            TaskFormMode::Edit(_) => " Edit card ",
            TaskFormMode::CreateFromAnnotations(_) => " New card from annotations ",
        };
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(self.theme.focus())
            .padding(Padding::horizontal(1));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        let [
            title_area,
            description_area,
            priority_area,
            column_area,
            error_area,
            actions_area,
        ] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(3),
            Constraint::Length(if form.target_column.is_some() { 3 } else { 0 }),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(inner);

        let title_active = matches!(self.active_input, Some(InputType::CreatingTaskTitle));
        let title_scroll = form
            .title
            .visual_scroll(title_area.width.saturating_sub(3) as usize);
        frame.render_widget(
            Paragraph::new(form.title.value())
                .scroll((0, title_scroll as u16))
                .block(
                    Block::bordered()
                        .title(" Title ")
                        .border_style(if title_active {
                            self.theme.focus()
                        } else {
                            Style::default().fg(self.theme.secondary)
                        }),
                ),
            title_area,
        );
        if title_active {
            let cursor = form.title.visual_cursor().max(title_scroll) - title_scroll + 1;
            frame.set_cursor_position((title_area.x + cursor as u16, title_area.y + 1));
        }

        let description_active =
            matches!(self.active_input, Some(InputType::CreatingTaskDescription));
        form.description
            .set_block(Block::bordered().title(" Description ").border_style(
                if description_active {
                    self.theme.focus()
                } else {
                    Style::default().fg(self.theme.secondary)
                },
            ));
        form.description.set_cursor_style(if description_active {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        });
        frame.render_widget(&form.description, description_area);

        let priority_active = matches!(self.active_input, Some(InputType::CreatingTaskPriority));
        let priority_block =
            Block::bordered()
                .title(" Priority ")
                .border_style(if priority_active {
                    self.theme.focus()
                } else {
                    Style::default().fg(self.theme.secondary)
                });
        let priority_inner = priority_block.inner(priority_area);
        frame.render_widget(priority_block, priority_area);
        let priority_line = [TaskPriority::LOW, TaskPriority::MEDIUM, TaskPriority::HIGH]
            .into_iter()
            .flat_map(|priority| {
                let style = if form.priority == priority {
                    match priority {
                        TaskPriority::HIGH => Style::default()
                            .fg(self.theme.error)
                            .add_modifier(Modifier::REVERSED),
                        TaskPriority::MEDIUM => Style::default()
                            .fg(self.theme.warning)
                            .add_modifier(Modifier::REVERSED),
                        TaskPriority::LOW => Style::default()
                            .fg(self.theme.focus)
                            .add_modifier(Modifier::REVERSED),
                    }
                } else {
                    Style::default().fg(self.theme.secondary)
                };
                [
                    Span::styled(format!(" {} ", priority.value()), style),
                    Span::raw("  "),
                ]
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(Line::from(priority_line)).alignment(Alignment::Center),
            priority_inner,
        );
        if let Some(column_index) = form.target_column {
            let column_active = matches!(self.active_input, Some(InputType::CreatingTaskColumn));
            let block = Block::bordered()
                .title(" Column ")
                .border_style(if column_active {
                    self.theme.focus()
                } else {
                    Style::default().fg(self.theme.secondary)
                });
            let inner = block.inner(column_area);
            frame.render_widget(block, column_area);
            let name = self
                .columns
                .get(column_index)
                .map(|column| column.name.as_str())
                .unwrap_or("Unknown");
            frame.render_widget(
                Paragraph::new(format!("←  {name}  →")).alignment(Alignment::Center),
                inner,
            );
        }
        if let Some(error) = &form.error {
            frame.render_widget(
                Paragraph::new(error.as_str()).style(Style::default().fg(self.theme.error)),
                error_area,
            );
        }
        frame.render_widget(
            Paragraph::new(if form.target_column.is_some() {
                "Tab next field  ←/→ priority/column  Ctrl+S save  Esc cancel"
            } else {
                "Tab next field  ←/→ priority  Ctrl+S save  Esc cancel"
            })
            .style(Style::default().fg(self.theme.secondary)),
            actions_area,
        );
    }
}
