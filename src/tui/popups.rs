use tui_input::Input;

pub(super) struct CreateColumnPopup {
    pub input: Input,
    pub error: Option<String>,
}

impl CreateColumnPopup {
    pub fn new() -> Self {
        Self {
            input: Input::default(),
            error: None,
        }
    }
}
