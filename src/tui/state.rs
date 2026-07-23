pub(super) enum InputMode {
    Normal,
    Editing,
    Visual,
}

#[allow(clippy::upper_case_acronyms)]
pub(super) enum VisualSelectionDirection {
    LEFT,
    RIGHT,
    UP,
    DOWN,
}

pub(super) enum Popup {
    DeleteColumn,
    EditTask,
}

#[allow(clippy::enum_variant_names)]
pub(super) enum InputType {
    CreatingColumn,
    CreatingTaskTitle,
    CreatingTaskDescription,
    CreatingTaskPriority,
}
