use std::fmt;
use std::ops::Deref;
use std::vec;

use crate::cell::Cell;
use crate::Label;

/// A text element that has a size and can be rendered to the terminal.
pub trait Element: fmt::Debug {
    /// Get the size of the element, in rows and columns.
    fn size(&self) -> Size;

    #[must_use]
    /// Render the element as lines of text that can be printed.
    fn render(&self) -> Vec<Line>;

    /// Get the number of columns occupied by this element.
    fn columns(&self) -> usize {
        self.size().cols
    }

    /// Get the number of rows occupied by this element.
    fn rows(&self) -> usize {
        self.size().rows
    }

    /// Print this element to stdout.
    fn print(&self) {
        for line in self.render() {
            println!("{line}");
        }
    }

    #[must_use]
    /// Return a string representation of this element.
    fn display(&self) -> String {
        let mut out = String::new();
        for line in self.render() {
            out.extend(line.into_iter().map(|l| l.to_string()));
            out.push('\n');
        }
        out
    }
}

impl<'a> Element for Box<dyn Element + 'a> {
    fn size(&self) -> Size {
        self.deref().size()
    }

    fn render(&self) -> Vec<Line> {
        self.deref().render()
    }

    fn print(&self) {
        self.deref().print()
    }
}

impl<T: Element> Element for &T {
    fn size(&self) -> Size {
        self.deref().size()
    }

    fn render(&self) -> Vec<Line> {
        self.deref().render()
    }

    fn print(&self) {
        self.deref().print()
    }
}

/// A line of text that has styling and can be displayed.
#[derive(Clone, Default, Debug)]
pub struct Line {
    items: Vec<Label>,
}

impl Line {
    /// Create a new line.
    pub fn new(item: impl Into<Label>) -> Self {
        Self {
            items: vec![item.into()],
        }
    }

    pub fn spaced(items: impl IntoIterator<Item = Label>) -> Self {
        let mut line = Self::default();
        for item in items.into_iter() {
            line.push(item);
            line.push(Label::space());
        }
        line.items.pop();
        line
    }

    /// Add an item to this line.
    pub fn item(mut self, item: impl Into<Label>) -> Self {
        self.push(item);
        self
    }

    /// Add multiple items to this line.
    pub fn extend(mut self, items: impl IntoIterator<Item = Label>) -> Self {
        self.items.extend(items.into_iter());
        self
    }

    /// Add an item to this line.
    pub fn push(&mut self, item: impl Into<Label>) {
        self.items.push(item.into());
    }

    /// Pad this line to occupy the given width.
    pub fn pad(&mut self, width: usize) {
        let w = self.columns();

        if width > w {
            let pad = width - w;
            self.items.push(Label::new(" ".repeat(pad).as_str()));
        }
    }

    /// Truncate this line to the given width.
    pub fn truncate(&mut self, width: usize, delim: &str) {
        while self.columns() > width {
            let total = self.columns();

            if total - self.items.last().map_or(0, Cell::width) > width {
                self.items.pop();
            } else if let Some(item) = self.items.last_mut() {
                *item = item.truncate(width - (total - Cell::width(item)), delim);
            }
        }
    }

    pub fn space(mut self) -> Self {
        self.items.push(Label::space());
        self
    }
}

impl IntoIterator for Line {
    type Item = Label;
    type IntoIter = vec::IntoIter<Label>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl From<Label> for Line {
    fn from(label: Label) -> Self {
        Self { items: vec![label] }
    }
}

impl Element for Line {
    fn size(&self) -> Size {
        Size::new(self.items.iter().map(Cell::width).sum(), 1)
    }

    fn render(&self) -> Vec<Line> {
        vec![self.clone()]
    }
}

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for item in &self.items {
            write!(f, "{item}")?;
        }
        Ok(())
    }
}

/// Size of a text element, in columns and rows.
#[derive(Clone, Debug, Default, Copy, PartialEq, Eq)]
pub struct Size {
    /// Columns occupied.
    pub cols: usize,
    /// Rows occupied.
    pub rows: usize,
}

impl Size {
    /// Create a new [`Size`].
    pub fn new(cols: usize, rows: usize) -> Self {
        Self { cols, rows }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_truncate() {
        let line = Line::default().item("banana").item("peach").item("apple");

        let mut actual = line.clone();
        actual.truncate(9, "…");
        assert_eq!(actual.to_string(), "bananape…");

        let mut actual = line.clone();
        actual.truncate(7, "…");
        assert_eq!(actual.to_string(), "banana…");

        let mut actual = line.clone();
        actual.truncate(1, "…");
        assert_eq!(actual.to_string(), "…");

        let mut actual = line;
        actual.truncate(0, "…");
        assert_eq!(actual.to_string(), "");
    }
}
