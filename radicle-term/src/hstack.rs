use crate::{Constraint, Element, Line, Size};

/// Horizontal stack of [`Element`] objects that implements [`Element`].
#[derive(Default, Debug)]
pub struct HStack<'a> {
    elems: Vec<Box<dyn Element + 'a>>,
}

impl<'a> HStack<'a> {
    /// Add an element to the stack.
    pub fn child(mut self, child: impl Element + 'a) -> Self {
        self.push(child);
        self
    }

    pub fn push(&mut self, child: impl Element + 'a) {
        self.elems.push(Box::new(child));
    }
}

impl Element for HStack<'_> {
    fn size(&self, parent: Constraint) -> Size {
        let width = self.elems.iter().map(|c| c.columns(parent)).sum();
        let height = self.elems.iter().map(|c| c.rows(parent)).max().unwrap_or(0);

        Size::new(width, height)
    }

    fn render(&self, parent: Constraint) -> Vec<Line> {
        fn rearrange(input: Vec<Vec<Line>>) -> Vec<Line> {
            let max_len = input.iter().map(|v| v.len()).max().unwrap_or(0);

            (0..max_len)
                .map(|i| {
                    Line::default().extend(
                        input
                            .iter()
                            .filter_map(move |v| v.get(i))
                            .flat_map(|l| l.clone()),
                    )
                })
                .collect()
        }
        rearrange(self.elems.iter().map(|e| e.render(parent)).collect())
    }
}
