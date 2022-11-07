use std::fmt;

use crate::terminal as term;

pub struct TextBox {
    pub body: String,
    first: bool,
    last: bool,
}

impl TextBox {
    pub fn new(body: String) -> Self {
        Self {
            body,
            first: true,
            last: true,
        }
    }

    /// Is this text box the last one in the list?
    pub fn last(mut self, connect: bool) -> Self {
        self.last = connect;
        self
    }

    /// Is this text box the first one in the list?
    pub fn first(mut self, connect: bool) -> Self {
        self.first = connect;
        self
    }
}

impl fmt::Display for TextBox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut width = self
            .body
            .lines()
            .map(console::measure_text_width)
            .max()
            .unwrap_or(0)
            + 2;
        if term::width() < width + 2 {
            width = term::width() - 2
        }

        let (connector, header_width) = if !self.first {
            ("┴", width - 1)
        } else {
            ("", width)
        };
        writeln!(f, "┌{}{}┐", connector, "─".repeat(header_width))?;

        for l in self.body.lines() {
            writeln!(
                f,
                "│ {}│",
                console::pad_str(l, width - 1, console::Alignment::Left, Some("…"))
            )?;
        }

        let (connector, footer_width) = if !self.last {
            ("┬", width - 1)
        } else {
            ("", width)
        };

        writeln!(f, "└{}{}┘", connector, "─".repeat(footer_width))?;

        if !self.last {
            writeln!(f, " │")?;
        }
        Ok(())
    }
}
