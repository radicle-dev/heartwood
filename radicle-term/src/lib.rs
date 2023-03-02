pub mod ansi;
pub mod cell;
pub mod command;
pub mod editor;
pub mod format;
pub mod io;
pub mod spinner;
pub mod table;
pub mod textbox;

pub use ansi::{paint, Paint};
pub use editor::Editor;
pub use inquire::ui::Styled;
pub use io::*;
pub use spinner::{spinner, Spinner};
pub use table::Table;
pub use textbox::TextBox;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Interactive {
    Yes,
    No,
}

impl Default for Interactive {
    fn default() -> Self {
        Interactive::No
    }
}

impl Interactive {
    pub fn yes(&self) -> bool {
        (*self).into()
    }

    pub fn no(&self) -> bool {
        !self.yes()
    }
}

impl From<Interactive> for bool {
    fn from(c: Interactive) -> Self {
        match c {
            Interactive::Yes => true,
            Interactive::No => false,
        }
    }
}

impl From<bool> for Interactive {
    fn from(b: bool) -> Self {
        if b {
            Interactive::Yes
        } else {
            Interactive::No
        }
    }
}

pub fn style<T>(item: T) -> Paint<T> {
    paint(item)
}
