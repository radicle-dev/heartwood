use std::fmt;

use crate::{cell::Cell, Element, Line, Paint, Size};

/// A styled string that does not contain any `'\n'` and implements [`Element`] and [`Cell`].
#[derive(Clone, Default, Debug)]
pub struct Label(Paint<String>);

impl Label {
    /// Create a new label.
    pub fn new(s: &str) -> Self {
        Self(Paint::new(s.replace('\n', " ")))
    }

    /// Create a blank label.
    pub fn blank() -> Self {
        Self(Paint::default())
    }
}

impl Element for Label {
    fn size(&self) -> Size {
        Size::new(self.0.width(), 1)
    }

    fn render(&self) -> Vec<Line> {
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

impl From<Paint<String>> for Label {
    fn from(paint: Paint<String>) -> Self {
        Self(Paint {
            item: paint.item.replace('\n', " "),
            style: paint.style,
        })
    }
}

impl From<Paint<&str>> for Label {
    fn from(paint: Paint<&str>) -> Self {
        Self(Paint {
            item: paint.item.replace('\n', " "),
            style: paint.style,
        })
    }
}

impl From<String> for Label {
    fn from(value: String) -> Self {
        Label::from(value.as_str())
    }
}

impl From<&str> for Label {
    fn from(value: &str) -> Self {
        Self(Paint::new(value.replace('\n', " ")))
    }
}

/// Create a new label from a [`Paint`] object.
pub fn label(s: impl Into<Paint<String>>) -> Label {
    Label(s.into())
}
