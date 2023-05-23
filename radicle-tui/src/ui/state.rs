use tuirealm::tui::widgets::{ListState, TableState};

/// State that holds the index of a selected tab item and the count of all tab items.
/// The index can be increased and will start at 0, if length was reached.
#[derive(Clone, Default)]
pub struct TabState {
    pub selected: u16,
    pub len: u16,
}

impl TabState {
    pub fn incr_tab_index(&mut self, rewind: bool) {
        if self.selected + 1 < self.len {
            self.selected += 1;
        } else if rewind {
            self.selected = 0;
        }
    }
}

#[derive(Clone)]
pub struct ItemState {
    selected: Option<usize>,
    len: usize,
}

impl ItemState {
    pub fn new(len: usize) -> Self {
        Self {
            selected: Some(0),
            len,
        }
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn select_previous(&mut self) -> Option<usize> {
        let old_index = self.selected();
        let new_index = match old_index {
            Some(selected) if selected == 0 => Some(0),
            Some(selected) => Some(selected.saturating_sub(1)),
            None => Some(0),
        };

        if old_index != new_index {
            self.selected = new_index;
            self.selected()
        } else {
            None
        }
    }

    pub fn select_next(&mut self) -> Option<usize> {
        let old_index = self.selected();
        let new_index = match old_index {
            Some(selected) if selected >= self.len.saturating_sub(1) => {
                Some(self.len.saturating_sub(1))
            }
            Some(selected) => Some(selected.saturating_add(1)),
            None => Some(0),
        };

        if old_index != new_index {
            self.selected = new_index;
            self.selected()
        } else {
            None
        }
    }
}

impl From<&ItemState> for TableState {
    fn from(value: &ItemState) -> Self {
        let mut state = TableState::default();
        state.select(value.selected);
        state
    }
}

impl From<&ItemState> for ListState {
    fn from(value: &ItemState) -> Self {
        let mut state = ListState::default();
        state.select(value.selected);
        state
    }
}
