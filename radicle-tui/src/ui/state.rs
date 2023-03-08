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
