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

    pub fn request_delete_confirmation(&mut self, index: usize) -> bool {
        if self.pending_delete_index == Some(index) {
            self.pending_delete_index = None;
            true
        } else {
            self.pending_delete_index = Some(index);
            false
        }
    }
}
