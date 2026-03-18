#[derive(Debug, Clone, Default)]
pub struct UiState {
    pub selected_index: Option<usize>,
    pub pending_delete_index: Option<usize>,
}

impl UiState {
    pub fn clear_selection(&mut self) {
        self.selected_index = None;
        self.pending_delete_index = None;
    }

    pub fn select(&mut self, index: usize) {
        self.selected_index = Some(index);
        self.pending_delete_index = None;
    }

    pub fn reset_pending_delete(&mut self) {
        self.pending_delete_index = None;
    }

    pub fn begin_delete_confirmation(&mut self, index: usize) {
        self.pending_delete_index = Some(index);
    }

    pub fn cancel_delete_confirmation(&mut self) {
        self.pending_delete_index = None;
    }

    pub fn take_confirmed_delete(&mut self) -> Option<usize> {
        self.pending_delete_index.take()
    }
}

#[cfg(test)]
mod tests {
    use super::UiState;

    #[test]
    fn delete_confirmation_lifecycle() {
        let mut state = UiState::default();
        state.begin_delete_confirmation(3);
        assert_eq!(state.pending_delete_index, Some(3));

        assert_eq!(state.take_confirmed_delete(), Some(3));
        assert_eq!(state.pending_delete_index, None);
    }

    #[test]
    fn cancel_delete_confirmation_clears_pending_value() {
        let mut state = UiState::default();
        state.begin_delete_confirmation(2);
        state.cancel_delete_confirmation();
        assert_eq!(state.pending_delete_index, None);
    }
}
