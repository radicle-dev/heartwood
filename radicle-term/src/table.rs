//! Print column-aligned text to the console.
//!
//! Example:
//! ```
//! use radicle_term::table::*;
//!
//! let mut t = Table::new(TableOptions::default());
//! t.push(["pest", "biological control"]);
//! t.push(["aphid", "lacewing"]);
//! t.push(["spider mite", "ladybug"]);
//! t.print();
//! ```
//! Output:
//! ``` plain
//! pest        biological control
//! aphid       ladybug
//! spider mite persimilis
//! ```
use std::fmt;

use crate as term;
use crate::cell::Cell;
use crate::{Label, Line, Size};

pub use crate::Element;

/// Used to specify maximum width or height.
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub struct Max {
    width: Option<usize>,
    height: Option<usize>,
}

#[derive(Debug)]
pub struct TableOptions {
    /// Whether the table should be allowed to overflow.
    pub overflow: bool,
    /// The maximum width and height.
    pub max: Max,
    /// Horizontal spacing between table cells.
    pub spacing: usize,
}

impl Default for TableOptions {
    fn default() -> Self {
        Self {
            overflow: false,
            max: Max::default(),
            spacing: 1,
        }
    }
}

#[derive(Debug)]
pub struct Table<const W: usize, T> {
    rows: Vec<[T; W]>,
    widths: [usize; W],
    width: usize,
    opts: TableOptions,
}

impl<const W: usize, T> Default for Table<W, T> {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            widths: [0; W],
            width: 0,
            opts: TableOptions::default(),
        }
    }
}

impl<const W: usize, T: Cell + fmt::Debug> Element for Table<W, T>
where
    T::Padded: Into<Label>,
{
    fn size(&self) -> Size {
        Table::size(self)
    }

    fn render(&self) -> Vec<Line> {
        let mut lines = Vec::new();
        let width = self.opts.max.width.or_else(term::columns);

        for row in &self.rows {
            let mut line = Line::default();

            for (i, cell) in row.iter().enumerate() {
                let pad = if i == row.len() - 1 {
                    0
                } else {
                    self.widths[i] + self.opts.spacing
                };
                line.push(cell.pad(pad));
            }

            if let Some(width) = width {
                line.truncate(width, "…");
            };
            lines.push(line);
        }
        lines
    }
}

impl<const W: usize, T: Cell> Table<W, T> {
    pub fn new(opts: TableOptions) -> Self {
        Self {
            rows: Vec::new(),
            widths: [0; W],
            width: 0,
            opts,
        }
    }

    pub fn size(&self) -> Size {
        Size::new(self.width, self.rows.len())
    }

    pub fn push(&mut self, row: [T; W]) {
        for (i, cell) in row.iter().enumerate() {
            self.widths[i] = self.widths[i].max(cell.width());
        }
        self.width =
            self.width.max(row.iter().map(Cell::width).sum()) + (W - 1) * self.opts.spacing;
        self.rows.push(row);
    }
}

#[cfg(test)]
mod test {
    use crate::Element;

    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_truncate() {
        assert_eq!("🍍".truncate(1, "…"), String::from("…"));
        assert_eq!("🍍".truncate(1, ""), String::from(""));
        assert_eq!("🍍🍍".truncate(2, "…"), String::from("…"));
        assert_eq!("🍍🍍".truncate(3, "…"), String::from("🍍…"));
        assert_eq!("🍍".truncate(1, "🍎"), String::from(""));
        assert_eq!("🍍".truncate(2, "🍎"), String::from("🍍"));
        assert_eq!("🍍🍍".truncate(3, "🍎"), String::from("🍎"));
        assert_eq!("🍍🍍🍍".truncate(4, "🍎"), String::from("🍍🍎"));
        assert_eq!("hello".truncate(3, "…"), String::from("he…"));
    }

    #[test]
    fn test_table() {
        let mut t = Table::new(TableOptions::default());

        t.push(["pineapple", "rosemary"]);
        t.push(["apples", "pears"]);

        #[rustfmt::skip]
        assert_eq!(
            t.display(),
            [
                "pineapple rosemary\n",
                "apples    pears\n"
            ].join("")
        );
    }

    #[test]
    fn test_table_truncate() {
        let mut t = Table::new(TableOptions {
            max: Max {
                width: Some(16),
                height: None,
            },
            ..TableOptions::default()
        });

        t.push(["pineapple", "rosemary"]);
        t.push(["apples", "pears"]);

        #[rustfmt::skip]
        assert_eq!(
            t.display(),
            [
                "pineapple rosem…\n",
                "apples    pears\n"
            ].join("")
        );
    }

    #[test]
    fn test_table_unicode() {
        let mut t = Table::new(TableOptions::default());

        t.push(["🍍pineapple", "__rosemary", "__sage"]);
        t.push(["__pears", "🍎apples", "🍌bananas"]);

        #[rustfmt::skip]
        assert_eq!(
            t.display(),
            [
                "🍍pineapple __rosemary __sage\n",
                "__pears     🍎apples   🍌bananas\n"
            ].join("")
        );
    }

    #[test]
    fn test_table_unicode_truncate() {
        let mut t = Table::new(TableOptions {
            max: Max {
                width: Some(16),
                height: None,
            },
            ..TableOptions::default()
        });

        t.push(["🍍pineapple", "__rosemary"]);
        t.push(["__pears", "🍎apples"]);

        #[rustfmt::skip]
        assert_eq!(
            t.display(),
            [
                "🍍pineapple __r…\n",
                "__pears     🍎a…\n"
            ].join("")
        );
    }
}
