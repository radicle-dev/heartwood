use unicode_width::UnicodeWidthStr;

use crate::{Element, Line, Paint, Size};

/// Text area.
///
/// A block of text that can contain multiple lines.
#[derive(Debug)]
pub struct TextArea(Paint<String>);

impl TextArea {
    /// Create a new text area.
    pub fn new(content: impl Into<Paint<String>>) -> Self {
        Self(content.into())
    }

    /// Get the lines of text in this text area.
    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.0.content().lines()
    }

    /// Box the text area.
    pub fn boxed(self) -> Box<dyn Element> {
        Box::new(self)
    }
}

impl Element for TextArea {
    fn size(&self) -> Size {
        let cols = self.lines().map(|l| l.width()).max().unwrap_or(0);
        let rows = self.lines().count();

        Size::new(cols, rows)
    }

    fn render(&self) -> Vec<Line> {
        self.lines()
            .map(|l| Line::new(Paint::new(l).with_style(self.0.style)))
            .collect()
    }
}

/// Create a new text area.
pub fn textarea(content: impl Into<Paint<String>>) -> TextArea {
    TextArea::new(content)
}
