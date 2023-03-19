use crate::{Element, Line, Size};

/// Horizontal stack of [`Element`] objects that implements [`Element`].
#[derive(Default, Debug)]
pub struct HStack<'a> {
    elems: Vec<Box<dyn Element + 'a>>,
    width: usize,
    height: usize,
}

impl<'a> HStack<'a> {
    /// Add an element to the stack.
    pub fn child(mut self, child: impl Element + 'a) -> Self {
        self.width += child.columns();
        self.height = self.height.max(child.rows());
        self.elems.push(Box::new(child));
        self
    }
}

impl<'a> Element for HStack<'a> {
    fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    fn render(&self) -> Vec<Line> {
        fn rearrange(input: Vec<Vec<Line>>) -> Vec<Line> {
            let max_len = input.iter().map(|v| v.len()).max().unwrap_or(0);

            (0..max_len)
                .map(|i| {
                    Line::default().extend(
                        input
                            .iter()
                            .filter_map(move |v| v.get(i))
                            .flat_map(|l| l.clone().into_iter()),
                    )
                })
                .collect()
        }
        rearrange(self.elems.iter().map(|e| e.render()).collect())
    }
}
