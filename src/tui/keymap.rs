use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum KeyContext {
    Board,
    Details,
    Form,
    Picker,
    Confirmation,
    Goto,
    Help,
    Annotations,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    Quit,
    ShowHelp,
    Close,
    MoveFocusLeft,
    MoveFocusRight,
    MoveFocusUp,
    MoveFocusDown,
    NewCard,
    NewColumn,
    EditCard,
    ViewCard,
    MarkDone,
    RequestArchive,
    RequestDeleteColumn,
    MoveCardsLeft,
    MoveCardsRight,
    MoveColumnLeft,
    MoveColumnRight,
    OpenMovePicker,
    ToggleSelection,
    EnterGoto,
    GotoFirstCard,
    GotoLastCard,
    GotoFirstColumn,
    GotoLastColumn,
    GotoVisibleCard,
    SaveForm,
    NextField,
    PreviousField,
    FormEnter,
    SelectPrevious,
    SelectNext,
    Accept,
    Confirm,
    Reject,
    ToggleChoice,
    ScrollUp,
    ScrollDown,
    SyncAnnotations,
    AnnotationPrevious,
    AnnotationNext,
    CreateFromAnnotations,
    IgnoreAnnotations,
    CycleAnnotationFilter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeySpec {
    Char(char),
    Ctrl(char),
    Code(KeyCode),
}

#[derive(Debug, Clone, Copy)]
struct Binding {
    context: KeyContext,
    key: KeySpec,
    action: Action,
    display: &'static str,
    description: &'static str,
    group: &'static str,
    show_in_footer: bool,
    show_in_help: bool,
}

macro_rules! binding {
    ($context:ident, $key:expr, $action:ident, $display:expr, $description:expr, $group:expr) => {
        Binding {
            context: KeyContext::$context,
            key: $key,
            action: Action::$action,
            display: $display,
            description: $description,
            group: $group,
            show_in_footer: true,
            show_in_help: true,
        }
    };
}

macro_rules! alias {
    ($context:ident, $key:expr, $action:ident) => {
        Binding {
            context: KeyContext::$context,
            key: $key,
            action: Action::$action,
            display: "",
            description: "",
            group: "",
            show_in_footer: false,
            show_in_help: false,
        }
    };
}

const BINDINGS: &[Binding] = &[
    binding!(
        Board,
        KeySpec::Char('h'),
        MoveFocusLeft,
        "h/l",
        "column",
        "Navigation"
    ),
    binding!(
        Board,
        KeySpec::Char('j'),
        MoveFocusDown,
        "j/k",
        "card",
        "Navigation"
    ),
    alias!(Board, KeySpec::Char('k'), MoveFocusUp),
    alias!(Board, KeySpec::Char('l'), MoveFocusRight),
    alias!(Board, KeySpec::Code(KeyCode::Left), MoveFocusLeft),
    alias!(Board, KeySpec::Code(KeyCode::Right), MoveFocusRight),
    alias!(Board, KeySpec::Code(KeyCode::Up), MoveFocusUp),
    alias!(Board, KeySpec::Code(KeyCode::Down), MoveFocusDown),
    alias!(Board, KeySpec::Code(KeyCode::Tab), MoveFocusRight),
    alias!(Board, KeySpec::Code(KeyCode::BackTab), MoveFocusLeft),
    binding!(Board, KeySpec::Char('n'), NewCard, "n", "new card", "Cards"),
    binding!(
        Board,
        KeySpec::Char('N'),
        NewColumn,
        "N",
        "new column",
        "Columns"
    ),
    binding!(Board, KeySpec::Char('e'), EditCard, "e", "edit", "Cards"),
    binding!(
        Board,
        KeySpec::Code(KeyCode::Enter),
        ViewCard,
        "Enter",
        "details",
        "Cards"
    ),
    binding!(Board, KeySpec::Char('d'), MarkDone, "d", "done", "Cards"),
    binding!(
        Board,
        KeySpec::Char('x'),
        RequestArchive,
        "x",
        "archive",
        "Cards"
    ),
    binding!(
        Board,
        KeySpec::Char('X'),
        RequestDeleteColumn,
        "X",
        "delete column",
        "Columns"
    ),
    binding!(
        Board,
        KeySpec::Char('['),
        MoveColumnLeft,
        "[/]",
        "reorder columns",
        "Columns"
    ),
    alias!(Board, KeySpec::Char(']'), MoveColumnRight),
    binding!(
        Board,
        KeySpec::Char('H'),
        MoveCardsLeft,
        "H/L",
        "move",
        "Movement"
    ),
    alias!(Board, KeySpec::Char('L'), MoveCardsRight),
    binding!(
        Board,
        KeySpec::Char('m'),
        OpenMovePicker,
        "m",
        "move to",
        "Movement"
    ),
    binding!(
        Board,
        KeySpec::Char(' '),
        ToggleSelection,
        "Space",
        "select",
        "Selection"
    ),
    binding!(
        Board,
        KeySpec::Char('g'),
        EnterGoto,
        "g",
        "goto",
        "Navigation"
    ),
    binding!(Board, KeySpec::Char('?'), ShowHelp, "?", "help", "Global"),
    binding!(
        Board,
        KeySpec::Ctrl('r'),
        SyncAnnotations,
        "Ctrl+R",
        "sync annotations",
        "Global"
    ),
    binding!(
        Board,
        KeySpec::Code(KeyCode::Esc),
        Close,
        "Esc",
        "clear",
        "Global"
    ),
    binding!(Board, KeySpec::Char('q'), Quit, "q", "quit", "Global"),
    binding!(
        Details,
        KeySpec::Char('h'),
        MoveFocusLeft,
        "h/j/k/l",
        "navigate board",
        "Details"
    ),
    alias!(Details, KeySpec::Char('j'), MoveFocusDown),
    alias!(Details, KeySpec::Char('k'), MoveFocusUp),
    alias!(Details, KeySpec::Char('l'), MoveFocusRight),
    alias!(Details, KeySpec::Code(KeyCode::Left), MoveFocusLeft),
    alias!(Details, KeySpec::Code(KeyCode::Right), MoveFocusRight),
    alias!(Details, KeySpec::Code(KeyCode::Up), MoveFocusUp),
    alias!(Details, KeySpec::Code(KeyCode::Down), MoveFocusDown),
    binding!(
        Details,
        KeySpec::Char('['),
        MoveColumnLeft,
        "[/]",
        "reorder columns",
        "Details"
    ),
    alias!(Details, KeySpec::Char(']'), MoveColumnRight),
    binding!(
        Details,
        KeySpec::Ctrl('u'),
        ScrollUp,
        "Ctrl+u/d",
        "scroll description",
        "Details"
    ),
    alias!(Details, KeySpec::Ctrl('d'), ScrollDown),
    binding!(
        Details,
        KeySpec::Char('e'),
        EditCard,
        "e",
        "edit",
        "Details"
    ),
    binding!(
        Details,
        KeySpec::Char('d'),
        MarkDone,
        "d",
        "done",
        "Details"
    ),
    binding!(
        Details,
        KeySpec::Char('x'),
        RequestArchive,
        "x",
        "archive",
        "Details"
    ),
    binding!(Details, KeySpec::Char('?'), ShowHelp, "?", "help", "Global"),
    binding!(
        Details,
        KeySpec::Code(KeyCode::Esc),
        Close,
        "Esc",
        "close",
        "Global"
    ),
    binding!(Form, KeySpec::Ctrl('s'), SaveForm, "Ctrl+S", "save", "Form"),
    binding!(
        Form,
        KeySpec::Code(KeyCode::Tab),
        NextField,
        "Tab",
        "next",
        "Form"
    ),
    binding!(
        Form,
        KeySpec::Code(KeyCode::BackTab),
        PreviousField,
        "Shift+Tab",
        "previous",
        "Form"
    ),
    binding!(
        Form,
        KeySpec::Code(KeyCode::Enter),
        FormEnter,
        "Enter",
        "advance/new line",
        "Form"
    ),
    binding!(
        Form,
        KeySpec::Code(KeyCode::Esc),
        Close,
        "Esc",
        "cancel",
        "Form"
    ),
    binding!(
        Picker,
        KeySpec::Char('k'),
        SelectPrevious,
        "j/k",
        "choose",
        "Picker"
    ),
    alias!(Picker, KeySpec::Char('j'), SelectNext),
    alias!(Picker, KeySpec::Code(KeyCode::Up), SelectPrevious),
    alias!(Picker, KeySpec::Code(KeyCode::Down), SelectNext),
    binding!(
        Picker,
        KeySpec::Code(KeyCode::Enter),
        Confirm,
        "Enter",
        "move",
        "Picker"
    ),
    binding!(
        Picker,
        KeySpec::Code(KeyCode::Esc),
        Close,
        "Esc",
        "cancel",
        "Picker"
    ),
    binding!(
        Confirmation,
        KeySpec::Char('y'),
        Accept,
        "y",
        "confirm",
        "Confirmation"
    ),
    binding!(
        Confirmation,
        KeySpec::Char('n'),
        Reject,
        "n",
        "cancel",
        "Confirmation"
    ),
    alias!(Confirmation, KeySpec::Code(KeyCode::Enter), Confirm),
    alias!(Confirmation, KeySpec::Code(KeyCode::Esc), Reject),
    binding!(
        Confirmation,
        KeySpec::Char('h'),
        ToggleChoice,
        "h/l/Tab",
        "change choice",
        "Confirmation"
    ),
    alias!(Confirmation, KeySpec::Char('l'), ToggleChoice),
    alias!(Confirmation, KeySpec::Code(KeyCode::Left), ToggleChoice),
    alias!(Confirmation, KeySpec::Code(KeyCode::Right), ToggleChoice),
    alias!(Confirmation, KeySpec::Code(KeyCode::Tab), ToggleChoice),
    alias!(Confirmation, KeySpec::Code(KeyCode::BackTab), ToggleChoice),
    binding!(
        Goto,
        KeySpec::Char('w'),
        GotoVisibleCard,
        "gw",
        "visible card",
        "Goto"
    ),
    binding!(
        Goto,
        KeySpec::Char('g'),
        GotoFirstCard,
        "gg",
        "first card",
        "Goto"
    ),
    binding!(
        Goto,
        KeySpec::Char('e'),
        GotoLastCard,
        "ge",
        "last card",
        "Goto"
    ),
    binding!(
        Goto,
        KeySpec::Char('h'),
        GotoFirstColumn,
        "gh",
        "first column",
        "Goto"
    ),
    binding!(
        Goto,
        KeySpec::Char('l'),
        GotoLastColumn,
        "gl",
        "last column",
        "Goto"
    ),
    binding!(
        Goto,
        KeySpec::Code(KeyCode::Esc),
        Close,
        "Esc",
        "cancel",
        "Goto"
    ),
    binding!(Help, KeySpec::Char('k'), ScrollUp, "j/k", "scroll", "Help"),
    alias!(Help, KeySpec::Char('j'), ScrollDown),
    alias!(Help, KeySpec::Code(KeyCode::Up), ScrollUp),
    alias!(Help, KeySpec::Code(KeyCode::Down), ScrollDown),
    binding!(Help, KeySpec::Char('?'), Close, "?", "close", "Help"),
    binding!(
        Help,
        KeySpec::Code(KeyCode::Esc),
        Close,
        "Esc",
        "close",
        "Help"
    ),
    binding!(
        Annotations,
        KeySpec::Char('k'),
        AnnotationPrevious,
        "j/k",
        "navigate",
        "Annotations"
    ),
    alias!(Annotations, KeySpec::Char('j'), AnnotationNext),
    alias!(Annotations, KeySpec::Code(KeyCode::Up), AnnotationPrevious),
    alias!(Annotations, KeySpec::Code(KeyCode::Down), AnnotationNext),
    binding!(
        Annotations,
        KeySpec::Char(' '),
        ToggleSelection,
        "Space",
        "select",
        "Annotations"
    ),
    binding!(
        Annotations,
        KeySpec::Char('c'),
        CreateFromAnnotations,
        "c",
        "create card",
        "Annotations"
    ),
    binding!(
        Annotations,
        KeySpec::Char('i'),
        IgnoreAnnotations,
        "i",
        "ignore",
        "Annotations"
    ),
    binding!(
        Annotations,
        KeySpec::Char('f'),
        CycleAnnotationFilter,
        "f",
        "filter",
        "Annotations"
    ),
    binding!(
        Annotations,
        KeySpec::Ctrl('r'),
        SyncAnnotations,
        "Ctrl+R",
        "rescan",
        "Annotations"
    ),
    binding!(
        Annotations,
        KeySpec::Code(KeyCode::Esc),
        Close,
        "Esc",
        "close",
        "Annotations"
    ),
];

pub(super) fn resolve(context: KeyContext, key: KeyEvent) -> Option<Action> {
    BINDINGS
        .iter()
        .find(|binding| binding.context == context && binding.key.matches(key))
        .map(|binding| binding.action)
}

pub(super) fn footer_hints(context: KeyContext) -> String {
    let preferred_actions: &[Action] = match context {
        KeyContext::Board => &[
            Action::NewCard,
            Action::EditCard,
            Action::MoveCardsLeft,
            Action::MoveColumnLeft,
            Action::OpenMovePicker,
            Action::ToggleSelection,
            Action::EnterGoto,
            Action::ShowHelp,
            Action::SyncAnnotations,
            Action::Quit,
        ],
        KeyContext::Annotations => &[
            Action::AnnotationPrevious,
            Action::ToggleSelection,
            Action::CreateFromAnnotations,
            Action::IgnoreAnnotations,
            Action::CycleAnnotationFilter,
            Action::SyncAnnotations,
            Action::Close,
        ],
        _ => &[],
    };
    BINDINGS
        .iter()
        .filter(|binding| {
            binding.context == context
                && binding.show_in_footer
                && (preferred_actions.is_empty() || preferred_actions.contains(&binding.action))
        })
        .map(|binding| format!("{} {}", binding.display, binding.description))
        .collect::<Vec<_>>()
        .join("  ")
}

pub(super) fn help_lines() -> Vec<String> {
    let mut lines = Vec::new();
    for group in [
        "Navigation",
        "Cards",
        "Movement",
        "Selection",
        "Columns",
        "Global",
    ] {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(group.to_owned());
        lines.extend(
            BINDINGS
                .iter()
                .filter(|binding| {
                    binding.context == KeyContext::Board
                        && binding.show_in_help
                        && binding.group == group
                })
                .map(|binding| format!("  {:<12} {}", binding.display, binding.description)),
        );
    }

    lines.push(String::new());
    lines.push("Goto".to_owned());
    lines.extend(
        BINDINGS
            .iter()
            .filter(|binding| binding.context == KeyContext::Goto && binding.show_in_help)
            .map(|binding| format!("  {:<12} {}", binding.display, binding.description)),
    );
    lines.push(String::new());
    lines.push("Forms".to_owned());
    lines.extend(
        BINDINGS
            .iter()
            .filter(|binding| binding.context == KeyContext::Form && binding.show_in_help)
            .map(|binding| format!("  {:<12} {}", binding.display, binding.description)),
    );
    lines
}

impl KeySpec {
    fn matches(self, event: KeyEvent) -> bool {
        match self {
            Self::Char(expected) => {
                matches!(event.code, KeyCode::Char(actual) if actual == expected)
                    && !event
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            }
            Self::Ctrl(expected) => {
                matches!(event.code, KeyCode::Char(actual) if actual.eq_ignore_ascii_case(&expected))
                    && event.modifiers.contains(KeyModifiers::CONTROL)
            }
            Self::Code(expected) => event.code == expected,
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyEventKind;

    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn resolves_primary_board_keys_and_navigation_aliases() {
        assert_eq!(
            resolve(KeyContext::Board, key(KeyCode::Char('h'))),
            Some(Action::MoveFocusLeft)
        );
        assert_eq!(
            resolve(KeyContext::Board, key(KeyCode::Left)),
            Some(Action::MoveFocusLeft)
        );
        assert_eq!(
            resolve(KeyContext::Board, key(KeyCode::Tab)),
            Some(Action::MoveFocusRight)
        );
        assert_eq!(
            resolve(KeyContext::Board, key(KeyCode::Char('H'))),
            Some(Action::MoveCardsLeft)
        );
        assert_eq!(
            resolve(KeyContext::Board, key(KeyCode::Char('['))),
            Some(Action::MoveColumnLeft)
        );
        assert_eq!(
            resolve(KeyContext::Board, key(KeyCode::Char(']'))),
            Some(Action::MoveColumnRight)
        );
        assert_eq!(
            resolve(KeyContext::Details, key(KeyCode::Char('['))),
            Some(Action::MoveColumnLeft)
        );
    }

    #[test]
    fn old_action_keys_are_not_bound() {
        for old in ['t', 'c', 's', 'S', 'D', 'i', 'v'] {
            assert_eq!(resolve(KeyContext::Board, key(KeyCode::Char(old))), None);
        }
        assert_eq!(resolve(KeyContext::Board, key(KeyCode::Backspace)), None);
    }

    #[test]
    fn contexts_do_not_leak_commands() {
        assert_eq!(resolve(KeyContext::Details, key(KeyCode::Char('q'))), None);
        assert_eq!(
            resolve(KeyContext::Goto, key(KeyCode::Char('w'))),
            Some(Action::GotoVisibleCard)
        );
        assert_eq!(
            resolve(KeyContext::Picker, key(KeyCode::Char('j'))),
            Some(Action::SelectNext)
        );
    }

    #[test]
    fn ctrl_s_is_only_a_form_save_command() {
        let save = KeyEvent {
            code: KeyCode::Char('s'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };
        assert_eq!(resolve(KeyContext::Form, save), Some(Action::SaveForm));
        assert_eq!(resolve(KeyContext::Board, save), None);
    }

    #[test]
    fn ctrl_r_syncs_annotations_but_plain_s_does_nothing() {
        let sync = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL);
        assert_eq!(
            resolve(KeyContext::Board, sync),
            Some(Action::SyncAnnotations)
        );
        assert_eq!(
            resolve(KeyContext::Annotations, sync),
            Some(Action::SyncAnnotations)
        );
        assert_eq!(resolve(KeyContext::Board, key(KeyCode::Char('s'))), None);
    }

    #[test]
    fn help_and_footer_are_derived_from_bindings() {
        assert!(footer_hints(KeyContext::Board).contains("n new card"));
        assert!(footer_hints(KeyContext::Board).contains("[/] reorder columns"));
        assert!(footer_hints(KeyContext::Goto).contains("gw visible card"));
        let help = help_lines().join("\n");
        assert!(help.contains("Navigation"));
        assert!(help.contains("Ctrl+S"));
        assert!(help.contains("Ctrl+R"));
        assert!(help.contains("[/]"));
        assert!(!help.contains("t new card"));
    }
}
