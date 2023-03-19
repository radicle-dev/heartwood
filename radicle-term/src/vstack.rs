use crate::{Color, Element, Label, Line, Paint, Size};

/// Options for [`VStack`].
#[derive(Default, Debug)]
pub struct VStackOptions {
    border: Option<Color>,
}

/// Vertical stack of [`Element`] objects that implements [`Element`].
#[derive(Default, Debug)]
pub struct VStack<'a> {
    elems: Vec<Box<dyn Element + 'a>>,
    opts: VStackOptions,
    width: usize,
    height: usize,
}

impl<'a> VStack<'a> {
    /// Add an element to the stack.
    pub fn child(mut self, child: impl Element + 'a) -> Self {
        self.width = self.width.max(child.columns());
        self.height += child.rows();
        self.elems.push(Box::new(child));
        self
    }

    /// Add a blank line to the stack.
    pub fn blank(self) -> Self {
        self.child(Label::blank())
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

        for elem in &self.elems {
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
