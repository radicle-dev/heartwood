use crate::{Color, Element, Label, Line, Paint, Size};

/// Options for [`VStack`].
#[derive(Default, Debug)]
pub struct VStackOptions {
    border: Option<Color>,
}

/// A vertical stack row.
#[derive(Default, Debug)]
enum Row<'a> {
    Element(Box<dyn Element + 'a>),
    #[default]
    Dividier,
}
/// Vertical stack of [`Element`] objects that implements [`Element`].
#[derive(Default, Debug)]
pub struct VStack<'a> {
    rows: Vec<Row<'a>>,
    opts: VStackOptions,
    width: usize,
    height: usize,
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
        self.height += 1;
        self
    }

    /// Add multiple elements to the stack.
    pub fn children<E: Element + 'a>(self, children: impl IntoIterator<Item = E>) -> Self {
        let mut vstack = self;

        for child in children.into_iter() {
            vstack = vstack.child(child);
        }
        vstack
    }

    /// Set or unset the outer border.
    pub fn border(mut self, color: Option<Color>) -> Self {
        self.opts.border = color;
        self
    }

    /// Add an element to the stack.
    pub fn push(&mut self, child: impl Element + 'a) {
        self.width = self.width.max(child.columns());
        self.height += child.rows();
        self.rows.push(Row::Element(Box::new(child)));
    }
}

impl<'a> Element for VStack<'a> {
    fn size(&self) -> Size {
        if self.opts.border.is_some() {
            Size::new(self.width + 4, self.height + 2)
        } else {
            Size::new(self.width, self.height)
        }
    }

    fn render(&self) -> Vec<Line> {
        let mut lines = Vec::new();

        if let Some(color) = self.opts.border {
            lines.push(
                Line::default()
                    .item(Paint::new("╭").fg(color))
                    .item(Paint::new("─".repeat(self.width + 2)).fg(color))
                    .item(Paint::new("╮").fg(color)),
            );
        }

        for row in &self.rows {
            match row {
                Row::Element(elem) => {
                    for mut line in elem.render() {
                        if let Some(color) = self.opts.border {
                            line.pad(self.width);
                            lines.push(
                                Line::default()
                                    .item(Paint::new("│ ").fg(color))
                                    .extend(line)
                                    .item(Paint::new(" │").fg(color)),
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
                                .item(Paint::new("─".repeat(self.width + 2)).fg(color))
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
                    .item(Paint::new("─".repeat(self.width + 2)).fg(color))
                    .item(Paint::new("╯").fg(color)),
            );
        }
        lines.into_iter().flat_map(|h| h.render()).collect()
    }
}
