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
use crate::{Color, Label, Line, Max, Paint, Size};

pub use crate::Element;

#[derive(Debug)]
pub struct TableOptions {
    /// Whether the table should be allowed to overflow.
    pub overflow: bool,
    /// The maximum width and height.
    pub max: Max,
    /// Horizontal spacing between table cells.
    pub spacing: usize,
    /// Table border.
    pub border: Option<Color>,
}

impl Default for TableOptions {
    fn default() -> Self {
        Self {
            overflow: false,
            max: Max::default(),
            spacing: 1,
            border: None,
        }
    }
}

impl TableOptions {
    pub fn bordered() -> Self {
        Self {
            border: Some(term::colors::FAINT),
            spacing: 3,
            ..Self::default()
        }
    }
}

#[derive(Debug)]
enum Row<const W: usize, T> {
    Data([T; W]),
    Divider,
}

#[derive(Debug)]
pub struct Table<const W: usize, T> {
    rows: Vec<Row<W, T>>,
    widths: [usize; W],
    opts: TableOptions,
}

impl<const W: usize, T> Default for Table<W, T> {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            widths: [0; W],
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
        let limits = self.limits();
        let width = limits.width;
        let border = self.opts.border;
        let inner = self.inner().limit(limits);
        let cols = inner.cols;

        if let Some(color) = border {
            lines.push(
                Line::default()
                    .item(Paint::new("╭").fg(color))
                    .item(Paint::new("─".repeat(cols)).fg(color))
                    .item(Paint::new("╮").fg(color)),
            );
        }

        for row in &self.rows {
            let mut line = Line::default();

            match row {
                Row::Data(cells) => {
                    if let Some(color) = border {
                        line.push(Paint::new("│ ").fg(color));
                    }
                    for (i, cell) in cells.iter().enumerate() {
                        let pad = if i == cells.len() - 1 {
                            if border.is_some() {
                                self.widths[i]
                            } else {
                                0
                            }
                        } else {
                            self.widths[i] + self.opts.spacing
                        };
                        line.push(cell.pad(pad));
                    }

                    if let Some(width) = width {
                        line.truncate(width, "…");
                    }
                    if let Some(color) = border {
                        line.push(Paint::new(" │").fg(color));
                    }
                    lines.push(line);
                }
                Row::Divider => {
                    if let Some(color) = border {
                        lines.push(
                            Line::default()
                                .item(Paint::new("├").fg(color))
                                .item(Paint::new("─".repeat(cols)).fg(color))
                                .item(Paint::new("┤").fg(color)),
                        );
                    } else {
                        lines.push(Line::default());
                    }
                }
            }
        }
        if let Some(color) = border {
            lines.push(
                Line::default()
                    .item(Paint::new("╰").fg(color))
                    .item(Paint::new("─".repeat(cols)).fg(color))
                    .item(Paint::new("╯").fg(color)),
            );
        }
        lines
    }
}

impl<const W: usize, T: Cell> Table<W, T> {
    pub fn new(opts: TableOptions) -> Self {
        Self {
            rows: Vec::new(),
            widths: [0; W],
            opts,
        }
    }

    pub fn size(&self) -> Size {
        self.outer()
    }

    pub fn divider(&mut self) {
        self.rows.push(Row::Divider);
    }

    pub fn limits(&self) -> Max {
        let width = self.opts.max.width.or_else(term::columns).map(|w| {
            if self.opts.border.is_some() {
                w - 2
            } else {
                w
            }
        });
        Max {
            width,
            height: None,
        }
    }

    pub fn push(&mut self, row: [T; W]) {
        for (i, cell) in row.iter().enumerate() {
            self.widths[i] = self.widths[i].max(cell.width());
        }
        self.rows.push(Row::Data(row));
    }

    fn inner(&self) -> Size {
        let mut cols = self.widths.iter().sum::<usize>() + (W - 1) * self.opts.spacing;
        let rows = self.rows.len();
        let limits = self.limits();

        // Account for inner spacing.
        if self.opts.border.is_some() {
            cols += 2;
        }
        Size::new(cols, rows).limit(limits)
    }

    fn outer(&self) -> Size {
        let mut inner = self.inner();

        // Account for outer borders.
        if self.opts.border.is_some() {
            inner.cols += 2;
            inner.rows += 2;
        }
        inner
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
    fn test_table_border() {
        let mut t = Table::new(TableOptions {
            border: Some(Color::Unset),
            spacing: 3,
            ..TableOptions::default()
        });

        t.push(["Country", "Population", "Code"]);
        t.divider();
        t.push(["France", "60M", "FR"]);
        t.push(["Switzerland", "7M", "CH"]);
        t.push(["Germany", "80M", "DE"]);

        let inner = t.inner();
        assert_eq!(inner.cols, 33);
        assert_eq!(inner.rows, 5);

        let outer = t.outer();
        assert_eq!(outer.cols, 35);
        assert_eq!(outer.rows, 7);

        assert_eq!(
            t.display(),
            r#"
╭─────────────────────────────────╮
│ Country       Population   Code │
├─────────────────────────────────┤
│ France        60M          FR   │
│ Switzerland   7M           CH   │
│ Germany       80M          DE   │
╰─────────────────────────────────╯
"#
            .trim_start()
        );
    }

    #[test]
    fn test_table_border_truncated() {
        let mut t = Table::new(TableOptions {
            border: Some(Color::Unset),
            spacing: 3,
            max: Max {
                width: Some(19),
                height: None,
            },
            ..TableOptions::default()
        });

        t.push(["Code", "Name"]);
        t.divider();
        t.push(["FR", "France"]);
        t.push(["CH", "Switzerland"]);
        t.push(["DE", "Germany"]);

        let inner = t.inner();
        assert_eq!(inner.cols, 17);
        assert_eq!(inner.rows, 5);

        let outer = t.outer();
        assert_eq!(outer.cols, 19);
        assert_eq!(outer.rows, 7);

        assert_eq!(
            t.display(),
            r#"
╭─────────────────╮
│ Code   Name     │
├─────────────────┤
│ FR     France   │
│ CH     Switzer… │
│ DE     Germany  │
╰─────────────────╯
"#
            .trim_start()
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
