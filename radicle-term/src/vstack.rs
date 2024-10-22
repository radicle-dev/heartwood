use crate::colors;
use crate::{Color, Constraint, Element, Label, Line, Paint, Size};

/// Options for [`VStack`].
#[derive(Debug)]
pub struct VStackOptions {
    border: Option<Color>,
    padding: usize,
}

impl Default for VStackOptions {
    fn default() -> Self {
        Self {
            border: None,
            padding: 1,
        }
    }
}

/// A vertical stack row.
#[derive(Default, Debug)]
enum Row<'a> {
    Element(Box<dyn Element + 'a>),
    #[default]
    Dividier,
}

impl<'a> Row<'a> {
    fn width(&self, c: Constraint) -> usize {
        match self {
            Self::Element(e) => e.columns(c),
            Self::Dividier => c.min.cols,
        }
    }

    fn height(&self, c: Constraint) -> usize {
        match self {
            Self::Element(e) => e.rows(c),
            Self::Dividier => 1,
        }
    }
}

/// Vertical stack of [`Element`] objects that implements [`Element`].
#[derive(Default, Debug)]
pub struct VStack<'a> {
    rows: Vec<Row<'a>>,
    opts: VStackOptions,
}

impl<'a> VStack<'a> {
    /// Add an element to the stack and return the stack.
    pub fn child(mut self, child: impl Element + 'a) -> Self {
        self.push(child);
        self
    }

    /// Add a blank line to the stack.
    pub fn blank(self) -> Self {
        self.child(Label::blank())
    }

    /// Add a horizontal divider.
    pub fn divider(mut self) -> Self {
        self.rows.push(Row::Dividier);
        self
    }

    /// Check if this stack is empty.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Add multiple elements to the stack.
    pub fn children<I>(self, children: I) -> Self
    where
        I: IntoIterator<Item = Box<dyn Element>>,
    {
        let mut vstack = self;

        for child in children.into_iter() {
            vstack = vstack.child(child);
        }
        vstack
    }

    /// Merge with another `VStack`.
    pub fn merge(mut self, other: Self) -> Self {
        for row in other.rows {
            self.rows.push(row);
        }
        self
    }

    /// Set or unset the outer border.
    pub fn border(mut self, color: Option<Color>) -> Self {
        self.opts.border = color;
        self
    }

    /// Set horizontal padding.
    pub fn padding(mut self, cols: usize) -> Self {
        self.opts.padding = cols;
        self
    }

    /// Add an element to the stack.
    pub fn push(&mut self, child: impl Element + 'a) {
        self.rows.push(Row::Element(Box::new(child)));
    }

    /// Box this element.
    pub fn boxed(self) -> Box<dyn Element + 'a> {
        Box::new(self)
    }

    /// Inner size.
    fn inner(&self, c: Constraint) -> Size {
        let mut outer = self.outer(c);

        if self.opts.border.is_some() {
            outer.cols -= 2;
            outer.rows -= 2;
        }
        outer
    }

    /// Outer size (includes borders).
    fn outer(&self, c: Constraint) -> Size {
        let padding = self.opts.padding * 2;
        let mut cols = self.rows.iter().map(|r| r.width(c)).max().unwrap_or(0) + padding;
        let mut rows = self.rows.iter().map(|r| r.height(c)).sum();

        // Account for outer borders.
        if self.opts.border.is_some() {
            cols += 2;
            rows += 2;
        }
        Size::new(cols, rows).constrain(c)
    }
}

impl<'a> Element for VStack<'a> {
    fn size(&self, parent: Constraint) -> Size {
        self.outer(parent)
    }

    fn render(&self, parent: Constraint) -> Vec<Line> {
        let mut lines = Vec::new();
        let padding = self.opts.padding;
        let inner = self.inner(parent);
        let child = Constraint::tight(inner.cols - padding * 2);

        if let Some(color) = self.opts.border {
            lines.push(
                Line::default()
                    .item(Paint::new("╭").fg(color))
                    .item(Paint::new("─".repeat(inner.cols)).fg(color))
                    .item(Paint::new("╮").fg(color)),
            );
        }

        for row in &self.rows {
            match row {
                Row::Element(elem) => {
                    for mut line in elem.render(child) {
                        line.pad(child.max.cols);

                        if let Some(color) = self.opts.border {
                            lines.push(
                                Line::default()
                                    .item(Paint::new(format!("│{}", " ".repeat(padding))).fg(color))
                                    .extend(line)
                                    .item(
                                        Paint::new(format!("{}│", " ".repeat(padding))).fg(color),
                                    ),
                            );
                        } else {
                            lines.push(line);
                        }
                    }
                }
                Row::Dividier => {
                    if let Some(color) = self.opts.border {
                        lines.push(
                            Line::default()
                                .item(Paint::new("├").fg(color))
                                .item(Paint::new("─".repeat(inner.cols)).fg(color))
                                .item(Paint::new("┤").fg(color)),
                        );
                    } else {
                        lines.push(Line::default());
                    }
                }
            }
        }

        if let Some(color) = self.opts.border {
            lines.push(
                Line::default()
                    .item(Paint::new("╰").fg(color))
                    .item(Paint::new("─".repeat(inner.cols)).fg(color))
                    .item(Paint::new("╯").fg(color)),
            );
        }
        lines.into_iter().flat_map(|h| h.render(child)).collect()
    }
}

/// Simple bordered vstack.
pub fn bordered<'a>(child: impl Element + 'a) -> VStack<'a> {
    VStack::default().border(Some(colors::FAINT)).child(child)
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_vstack() {
        let mut v = VStack::default().border(Some(Color::Unset)).padding(1);

        v.push(Line::new("banana"));
        v.push(Line::new("apple"));
        v.push(Line::new("abricot"));

        let constraint = Constraint::default();
        let outer = v.outer(constraint);
        assert_eq!(outer.cols, 11);
        assert_eq!(outer.rows, 5);

        let inner = v.inner(constraint);
        assert_eq!(inner.cols, 9);
        assert_eq!(inner.rows, 3);

        assert_eq!(
            v.display(constraint, &Context { ansi: false }),
            r#"
╭─────────╮
│ banana  │
│ apple   │
│ abricot │
╰─────────╯
"#
            .trim_start()
        );
    }

    #[test]
    fn test_vstack_maximize() {
        let mut v = VStack::default().border(Some(Color::Unset)).padding(1);

        v.push(Line::new("banana"));
        v.push(Line::new("apple"));
        v.push(Line::new("abricot"));

        let constraint = Constraint {
            min: Size::new(14, 0),
            max: Size::new(14, usize::MAX),
        };
        let outer = v.outer(constraint);
        assert_eq!(outer.cols, 14);
        assert_eq!(outer.rows, 5);

        let inner = v.inner(constraint);
        assert_eq!(inner.cols, 12);
        assert_eq!(inner.rows, 3);

        assert_eq!(
            v.display(constraint, &Context { ansi: false }),
            r#"
╭────────────╮
│ banana     │
│ apple      │
│ abricot    │
╰────────────╯
"#
            .trim_start()
        );
    }
}
