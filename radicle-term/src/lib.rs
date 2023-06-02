pub mod ansi;
pub mod cell;
pub mod colors;
pub mod command;
pub mod editor;
pub mod element;
pub mod format;
pub mod hstack;
pub mod io;
pub mod label;
pub mod spinner;
pub mod table;
pub mod textarea;
pub mod vstack;

pub use ansi::Color;
pub use ansi::{paint, Paint};
pub use editor::Editor;
pub use element::{Element, Line, Max, Size};
pub use hstack::HStack;
pub use inquire::ui::Styled;
pub use io::*;
pub use label::{label, Label};
pub use spinner::{spinner, spinner_to, Spinner};
pub use table::Table;
pub use textarea::{textarea, TextArea};
pub use vstack::{VStack, VStackOptions};

#[derive(Debug, PartialEq, Eq, Copy, Clone, Default)]
pub enum Interactive {
    Yes,
    #[default]
    No,
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
