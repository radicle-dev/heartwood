//! Print column-aligned text to the console.
//!
//! Example:
//! ```
//! use radicle_term::table::*;
//!
//! let mut t: Table<2, &str, &str> = Table::new(TableOptions::default());
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

use crate::cell::Cell;
use crate::{self as term, Style};
use crate::{Color, Constraint, Context, Line, Paint, Size};

pub use crate::Element;

#[derive(Debug, Default)]
pub enum TableDirection {
    /// Headers are shown in the first row, consecutive rows contain elements.
    /// For n headers and m elements, the table will have O(n) columns and O(m) rows.
    #[default]
    TopToBottom,
    /// Headers are shown in the first column, consecutive columns contain elements.
    /// For n headers and m elements, the table will have O(m) columns and O(n) rows.
    LeftToRight,
}

#[derive(Debug)]
pub struct TableOptions {
    /// Whether the table should be allowed to overflow.
    pub overflow: bool,
    /// Horizontal spacing between table cells.
    pub spacing: usize,
    /// Table border.
    pub border: Option<Color>,
    pub direction: TableDirection,
}

impl Default for TableOptions {
    fn default() -> Self {
        Self {
            overflow: false,
            spacing: 1,
            border: None,
            direction: Default::default(),
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
pub struct Table<const W: usize, T, H> {
    header: Option<[H; W]>,
    rows: Vec<Row<W, T>>,
    widths: [usize; W],
    opts: TableOptions,
}

impl<const W: usize, T, H> Default for Table<W, T, H> {
    fn default() -> Self {
        Self {
            header: None,
            rows: Vec::new(),
            widths: [0; W],
            opts: TableOptions::default(),
        }
    }
}

impl<
        'a,
        const W: usize,
        T: Cell<'a> + fmt::Debug + Send + Sync,
        H: Cell<'a> + fmt::Debug + Send + Sync,
    > Element for Table<W, T, H>
where
    T::Padded: Into<Line>,
    H::Padded: Into<Line>,
{
    fn size(&self, parent: Constraint) -> Size {
        Table::size(self, parent)
    }

    fn render(&self, parent: Constraint) -> Vec<Line> {
        let mut lines = Vec::new();
        let border = self.opts.border;
        let inner = self.inner(parent);
        let cols = inner.cols;

        // Don't print empty tables.
        if self.is_empty() {
            return lines;
        }

        if let Some(color) = border {
            lines.push(
                Line::default()
                    .item(Paint::new("╭").fg(color))
                    .item(Paint::new("─".repeat(cols)).fg(color))
                    .item(Paint::new("╮").fg(color)),
            );
        }

        if let Some(cells) = &self.header {
            let mut line = Line::default();

            if let Some(color) = border {
                line.push(Paint::new("│ ").fg(color));
            }
            for (i, cell) in cells.iter().enumerate() {
                let pad = if i == cells.len() - 1 {
                    0
                } else {
                    self.widths[i] + self.opts.spacing
                };
                line = line.extend(
                    cell.pad(pad)
                        .into()
                        .style(Style::default().bg(cell.background())),
                );
            }
            Line::pad(&mut line, cols);
            Line::truncate(&mut line, cols, "…");

            if let Some(color) = border {
                line.push(Paint::new(" │").fg(color));
            }
            lines.push(line);

            // Divider
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

        for row in &self.rows {
            let mut line = Line::default();

            match row {
                Row::Data(cells) => {
                    if let Some(color) = border {
                        line.push(Paint::new("│ ").fg(color));
                    }
                    for (i, cell) in cells.iter().enumerate() {
                        let pad = if i == cells.len() - 1 {
                            0
                        } else {
                            self.widths[i] + self.opts.spacing
                        };
                        line = line.extend(
                            cell.pad(pad)
                                .into()
                                .style(Style::default().bg(cell.background())),
                        );
                    }
                    Line::pad(&mut line, cols);
                    Line::truncate(&mut line, cols, "…");

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

impl<'a, const W: usize, T: Cell<'a>, H: Cell<'a>> Table<W, T, H> {
    pub fn new(opts: TableOptions) -> Self {
        Self {
            header: None,
            rows: Vec::new(),
            widths: [0; W],
            opts,
        }
    }

    pub fn size(&self, parent: Constraint) -> Size {
        self.outer(parent)
    }

    pub fn divider(&mut self) {
        self.rows.push(Row::Divider);
    }

    pub fn push(&mut self, row: [T; W]) {
        for (i, cell) in row.iter().enumerate() {
            self.widths[i] = self.widths[i].max(cell.width());
        }
        self.rows.push(Row::Data(row));
    }

    pub fn header(&mut self, row: [H; W]) {
        for (i, cell) in row.iter().enumerate() {
            self.widths[i] = self.widths[i].max(cell.width());
        }
        self.header = Some(row);
    }

    pub fn extend(&mut self, rows: impl IntoIterator<Item = [T; W]>) {
        for row in rows.into_iter() {
            self.push(row);
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.rows.iter().any(|r| matches!(r, Row::Data { .. }))
    }

    fn inner(&self, c: Constraint) -> Size {
        let mut outer = self.outer(c);

        if self.opts.border.is_some() {
            outer.cols -= 2;
            outer.rows -= 2;
        }
        outer
    }

    fn outer(&self, c: Constraint) -> Size {
        let mut cols = self.widths.iter().sum::<usize>() + (W - 1) * self.opts.spacing;
        let mut rows = self.rows.len();
        let padding = 2;

        // Account for outer borders.
        if self.opts.border.is_some() {
            cols += 2 + padding;
            rows += 2;
        }
        Size::new(cols, rows).constrain(c)
    }
}

impl<const W: usize, T: ToString, H: ToString> Table<W, Paint<T>, Paint<H>> {
    pub fn to_json(&self) -> serde_json::Value {
        let header = {
            match &self.header {
                Some(header) => header,
                _ => {
                    // TODO: Return array of arrays?
                    panic!("Cannot convert table to JSON. Expecting header.")
                }
            }
        };

        serde_json::Value::Array(
            self.rows[1..]
                .iter()
                .filter_map(|row| match row {
                    Row::Data(cells) => {
                        let mut obj = serde_json::Map::new();
                        header.iter().zip(cells.iter()).for_each(|(key, value)| {
                            obj.insert(
                                key.item.to_string(),
                                serde_json::Value::String(value.item.to_string()),
                            );
                        });
                        Some(serde_json::Value::Object(obj))
                    }
                    Row::Divider => None,
                })
                .collect(),
        )
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
        let mut t: Table<2, &str, Paint<String>> = Table::new(TableOptions::default());

        t.push(["pineapple", "rosemary"]);
        t.push(["apples", "pears"]);

        #[rustfmt::skip]
        assert_eq!(
            t.display_xx(Constraint::UNBOUNDED, &Context { ansi: false }),
            [
                "pineapple rosemary\n",
                "apples    pears   \n"
            ].join("")
        );
    }

    #[test]
    fn test_table_border() {
        let mut t: Table<3, &str, Paint<String>> = Table::new(TableOptions {
            border: Some(Color::Unset),
            spacing: 3,
            ..TableOptions::default()
        });

        t.push(["Country", "Population", "Code"]);
        t.divider();
        t.push(["France", "60M", "FR"]);
        t.push(["Switzerland", "7M", "CH"]);
        t.push(["Germany", "80M", "DE"]);

        let inner = t.inner(Constraint::UNBOUNDED);
        assert_eq!(inner.cols, 33);
        assert_eq!(inner.rows, 5);

        let outer = t.outer(Constraint::UNBOUNDED);
        assert_eq!(outer.cols, 35);
        assert_eq!(outer.rows, 7);

        assert_eq!(
            t.display_xx(Constraint::UNBOUNDED, &Context { ansi: false }),
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
        let mut t: Table<2, &str, Paint<String>> = Table::new(TableOptions {
            border: Some(Color::Unset),
            spacing: 3,
            ..TableOptions::default()
        });

        t.push(["Code", "Name"]);
        t.divider();
        t.push(["FR", "France"]);
        t.push(["CH", "Switzerland"]);
        t.push(["DE", "Germany"]);

        let constrain = Constraint::max(Size {
            cols: 19,
            rows: usize::MAX,
        });
        let outer = t.outer(constrain);
        assert_eq!(outer.cols, 19);
        assert_eq!(outer.rows, 7);

        let inner = t.inner(constrain);
        assert_eq!(inner.cols, 17);
        assert_eq!(inner.rows, 5);

        assert_eq!(
            t.display_xx(constrain, &Context { ansi: false }),
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
    fn test_table_border_maximized() {
        let mut t: Table<2, &str, Paint<String>> = Table::new(TableOptions {
            border: Some(Color::Unset),
            spacing: 3,
            ..TableOptions::default()
        });

        t.push(["Code", "Name"]);
        t.divider();
        t.push(["FR", "France"]);
        t.push(["CH", "Switzerland"]);
        t.push(["DE", "Germany"]);

        let constrain = Constraint::new(
            Size { cols: 26, rows: 0 },
            Size {
                cols: 26,
                rows: usize::MAX,
            },
        );
        let outer = t.outer(constrain);
        assert_eq!(outer.cols, 26);
        assert_eq!(outer.rows, 7);

        let inner = t.inner(constrain);
        assert_eq!(inner.cols, 24);
        assert_eq!(inner.rows, 5);

        assert_eq!(
            t.display_xx(constrain, &Context { ansi: false }),
            r#"
╭────────────────────────╮
│ Code   Name            │
├────────────────────────┤
│ FR     France          │
│ CH     Switzerland     │
│ DE     Germany         │
╰────────────────────────╯
"#
            .trim_start()
        );
    }

    #[test]
    fn test_table_truncate() {
        let mut t: Table<2, &str, Paint<String>> = Table::default();
        let constrain = Constraint::new(
            Size::MIN,
            Size {
                cols: 16,
                rows: usize::MAX,
            },
        );

        t.push(["pineapple", "rosemary"]);
        t.push(["apples", "pears"]);

        #[rustfmt::skip]
        assert_eq!(
            t.display_xx(constrain, &Context { ansi: false }),
            [
                "pineapple rosem…\n",
                "apples    pears \n"
            ].join("")
        );
    }

    #[test]
    fn test_table_unicode() {
        let mut t: Table<3, &str, Paint<String>> = Table::new(TableOptions::default());

        t.push(["🍍pineapple", "__rosemary", "__sage"]);
        t.push(["__pears", "🍎apples", "🍌bananas"]);

        #[rustfmt::skip]
        assert_eq!(
            t.display_xx(Constraint::UNBOUNDED, &Context { ansi: false }),
            [
                "🍍pineapple __rosemary __sage   \n",
                "__pears     🍎apples   🍌bananas\n"
            ].join("")
        );
    }

    #[test]
    fn test_table_unicode_truncate() {
        let mut t: Table<2, &str, Paint<String>> = Table::new(TableOptions {
            ..TableOptions::default()
        });
        let constrain = Constraint::max(Size {
            cols: 16,
            rows: usize::MAX,
        });
        t.push(["🍍pineapple", "__rosemary"]);
        t.push(["__pears", "🍎apples"]);

        #[rustfmt::skip]
        assert_eq!(
            t.display_xx(constrain, &Context { ansi: false }),
            [
                "🍍pineapple __r…\n",
                "__pears     🍎a…\n"
            ].join("")
        );
    }

    #[test]
    fn test_table_json() {
        let mut t = Table::new(TableOptions {
            border: Some(Color::Unset),
            spacing: 3,
            ..TableOptions::default()
        });

        #[derive(serde::Serialize)]
        struct Entry {
            #[serde(rename = "Country")]
            country: &'static str,
            #[serde(rename = "Population")]
            population: &'static str,
            #[serde(rename = "Code")]
            code: &'static str,
        }

        let entries = vec![
            Entry {
                country: "France",
                population: "60M",
                code: "FR",
            },
            Entry {
                country: "Switzerland",
                population: "7M",
                code: "CH",
            },
            Entry {
                country: "Germany",
                population: "80M",
                code: "DE",
            },
        ];

        t.header([
            term::format::tertiary("Country"),
            term::format::tertiary("Population"),
            term::format::tertiary("Code"),
        ]);
        t.divider();
        for entry in entries.iter() {
            t.push([
                term::format::default(entry.country),
                term::format::default(entry.population),
                term::format::default(entry.code),
            ]);
        }

        //Paint::disable();

        assert_eq!(t.to_json(), serde_json::json!(entries));
    }
}
