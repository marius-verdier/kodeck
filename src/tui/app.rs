mod board;
mod controller;
mod render;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use color_eyre::eyre::Result;
use crossterm::event;
use ratatui::DefaultTerminal;

use crate::domain::{CardId, Column, Task, UiStateFile, WorkspaceConfig};
use crate::workspace::{PrivateWorkspaceStore, SharedWorkspaceStore, WorkspaceContext};

use super::state::{
    ArchivedTask, ColumnFormState, ConfirmationState, DetailState, GotoLabelsState, InputMode,
    InputType, MovePickerState, StatusMessage, TaskFormState,
};
use super::ui::{UiTheme, ViewportState};

pub struct App<'a> {
    workspace_root: PathBuf,
    workspace_config: WorkspaceConfig,
    shared_store: SharedWorkspaceStore,
    private_store: PrivateWorkspaceStore,
    ui_state: UiStateFile,
    findings_count: usize,
    last_error: Option<String>,
    input_mode: InputMode,
    active_input: Option<InputType>,
    viewport: ViewportState,
    theme: UiTheme,
    selected_cards: HashSet<CardId>,
    confirmation: Option<ConfirmationState>,
    move_picker: Option<MovePickerState>,
    goto_pending: bool,
    goto_labels: Option<GotoLabelsState>,
    help_open: bool,
    help_scroll: usize,
    visible_cards: Vec<CardId>,
    status_message: Option<StatusMessage>,
    // COLUMNS MANAGEMENT
    columns: Vec<Column>,
    focused_column: usize,
    create_column_popup: Option<ColumnFormState>,
    // TASKS MANAGEMENT
    focused_task: usize,
    task_form: Option<TaskFormState<'a>>,
    detail: Option<DetailState>,
    archived_tasks: Vec<ArchivedTask>,
}

impl App<'_> {
    pub fn from_workspace(context: WorkspaceContext, findings_count: usize) -> Self {
        let mut warnings = context.warnings.clone();
        let mut columns: Vec<_> = context
            .config
            .columns
            .iter()
            .map(|column| Column::new(column.id.clone(), column.name.clone(), 50))
            .collect();
        let cards = &context.private_state.cards;
        let cards_by_id: HashMap<_, _> = cards.cards.iter().map(|card| (card.id, card)).collect();
        let mut placed = HashSet::new();

        for column in &mut columns {
            if let Some(card_ids) = cards.ordering.get(&column.id) {
                for card_id in card_ids {
                    if let Some(card) = cards_by_id.get(card_id)
                        && !card.archived
                    {
                        let order = column.tasks.len();
                        column.tasks.push(Task::from_local_card(card, order));
                        placed.insert(card.id);
                    }
                }
            }
        }

        for card in cards
            .cards
            .iter()
            .filter(|card| !card.archived && !placed.contains(&card.id))
        {
            let target = columns
                .iter()
                .position(|column| column.id == card.column_id)
                .unwrap_or(0);
            if !columns.iter().any(|column| column.id == card.column_id) {
                warnings.push(format!(
                    "card '{}' references missing column '{}'; displayed in '{}'",
                    card.title, card.column_id, columns[target].name
                ));
            }
            let order = columns[target].tasks.len();
            columns[target]
                .tasks
                .push(Task::from_local_card(card, order));
        }

        let archived_tasks: Vec<ArchivedTask> = cards
            .cards
            .iter()
            .filter(|card| card.archived)
            .map(|card| ArchivedTask {
                column_id: card.column_id.clone(),
                task: Task::from_local_card(card, 0),
            })
            .collect();

        let focused_column = context
            .private_state
            .state
            .selected_column_id
            .as_ref()
            .and_then(|id| columns.iter().position(|column| &column.id == id))
            .unwrap_or(0);
        let focused_task = context
            .private_state
            .state
            .selected_card_id
            .and_then(|id| {
                columns[focused_column]
                    .tasks
                    .iter()
                    .position(|task| task.id == id)
            })
            .unwrap_or(0);
        let shared_store = context.shared_store().clone();
        let private_store = context.private_store().clone();
        let ui_state = context.private_state.state.clone();
        let column_count = columns.len();
        let initial_status = warnings.first().cloned().map(StatusMessage::warn);

        Self {
            workspace_root: context.root,
            workspace_config: context.config,
            shared_store,
            private_store,
            ui_state,
            findings_count,
            last_error: None,
            input_mode: InputMode::Normal,
            active_input: None,
            columns,
            viewport: ViewportState {
                first_column: focused_column,
                card_offsets: vec![0; column_count],
            },
            theme: UiTheme::default(),
            selected_cards: HashSet::new(),
            confirmation: None,
            move_picker: None,
            goto_pending: false,
            goto_labels: None,
            help_open: false,
            help_scroll: 0,
            visible_cards: Vec::new(),
            status_message: initial_status,
            focused_column,
            focused_task,
            create_column_popup: None,
            task_form: None,
            detail: None,
            archived_tasks,
        }
    }

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        loop {
            terminal.draw(|frame| self.render(frame))?;
            if let Some(key) = event::read()?.as_key_press_event()
                && self.handle_key(key)
            {
                self.persist_ui_state();
                return Ok(());
            }
        }
    }
}
