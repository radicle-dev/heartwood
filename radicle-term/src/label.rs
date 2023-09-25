use std::fmt;

use crate::{cell::Cell, Color, Constraint, Element, Filled, Line, Paint, Size, Style};

/// A styled string that does not contain any `'\n'` and implements [`Element`] and [`Cell`].
#[derive(Clone, Default, Debug)]
pub struct Label(Paint<String>);

impl Label {
    /// Create a new label.
    pub fn new(s: &str) -> Self {
        Self(Paint::new(cleanup(s)))
    }

    /// Create a blank label.
    pub fn blank() -> Self {
        Self(Paint::default())
    }

    /// Get unstyled content.
    pub fn content(&self) -> &str {
        self.0.content()
    }

    /// Create a single space label
    pub fn space() -> Self {
        Self(Paint::new(" ".to_owned()))
    }

    /// Box the label.
    pub fn boxed(self) -> Box<dyn Element> {
        Box::new(self)
    }

    /// Color the label's foreground.
    pub fn fg(self, color: Color) -> Self {
        Self(self.0.fg(color))
    }

    /// Color the label's background.
    pub fn bg(self, color: Color) -> Self {
        Self(self.0.bg(color))
    }

    /// Style a label.
    pub fn style(self, style: Style) -> Self {
        Self(self.0.with_style(style))
    }

    /// Get inner paint object.
    pub fn paint(&self) -> &Paint<String> {
        &self.0
    }

    /// Return a filled cell from this label.
    pub fn filled(self, color: Color) -> Filled<Self> {
        Filled { item: self, color }
    }

    /// Wrap into a line.
    pub fn to_line(self) -> Line {
        Line::from(self)
    }
}

impl Element for Label {
    fn size(&self, _parent: Constraint) -> Size {
        Size::new(self.0.width(), 1)
    }

    fn render(&self, _parent: Constraint) -> Vec<Line> {
        vec![Line::new(self.clone())]
    }
}

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Cell for Label {
    type Padded = Self;
    type Truncated = Self;

    fn background(&self) -> Color {
        self.paint().style.background
    }

    fn pad(&self, width: usize) -> Self::Padded {
        Self(self.0.pad(width))
    }

    fn truncate(&self, width: usize, delim: &str) -> Self::Truncated {
        Self(self.0.truncate(width, delim))
    }

    fn width(&self) -> usize {
        Cell::width(&self.0)
    }
}

impl<D: fmt::Display> From<Paint<D>> for Label {
    fn from(paint: Paint<D>) -> Self {
        Self(Paint {
            item: cleanup(paint.item.to_string().as_str()),
            style: paint.style,
        })
    }
}

impl From<String> for Label {
    fn from(value: String) -> Self {
        Self::new(value.as_str())
    }
}

impl From<&str> for Label {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Create a new label from a [`Paint`] object.
pub fn label(s: impl Into<Paint<String>>) -> Label {
    Label::from(s.into())
}

/// Cleanup the input string for display as a label.
fn cleanup(input: &str) -> String {
    input
        .chars()
        .filter(|c| *c != '\u{fe0f}' && *c != '\n' && *c != '\r')
        .collect()
}
