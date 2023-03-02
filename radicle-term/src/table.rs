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
//! t.render();
//! ```
//! Output:
//! ``` plain
//! pest        biological control
//! aphid       ladybug
//! spider mite persimilis
//! ```
use std::io;

use crate as term;
use crate::cell::Cell;

/// Used to specify maximum width or height.
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub struct Max {
    width: Option<usize>,
    height: Option<usize>,
}

#[derive(Debug, Default)]
pub struct TableOptions {
    /// Whether the table should be allowed to overflow.
    pub overflow: bool,
    /// The maximum width and height.
    pub max: Max,
}

#[derive(Debug)]
pub struct Table<const W: usize> {
    rows: Vec<[String; W]>,
    widths: [usize; W],
    opts: TableOptions,
}

impl<const W: usize> Default for Table<W> {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            widths: [0; W],
            opts: TableOptions::default(),
        }
    }
}

impl<const W: usize> Table<W> {
    pub fn new(opts: TableOptions) -> Self {
        Self {
            rows: Vec::new(),
            widths: [0; W],
            opts,
        }
    }

    pub fn push(&mut self, row: [impl Cell; W]) {
        let row = row.map(|s| s.to_string());
        for (i, cell) in row.iter().enumerate() {
            self.widths[i] = self.widths[i].max(cell.width());
        }
        self.rows.push(row);
    }

    pub fn render(self) {
        self.write(io::stdout()).ok();
    }

    pub fn write<T: io::Write>(self, mut writer: T) -> io::Result<()> {
        let width = self.opts.max.width.or_else(term::columns);

        for row in &self.rows {
            let mut output = String::new();
            let cells = row.len();

            for (i, cell) in row.iter().enumerate() {
                if i == cells - 1 || self.opts.overflow {
                    output.push_str(cell.to_string().as_str());
                } else {
                    output.push_str(cell.pad(self.widths[i]).as_str());
                    output.push(' ');
                }
            }

            let output = output.trim_end();
            writeln!(
                writer,
                "{}",
                if let Some(width) = width {
                    output.truncate(width, "…")
                } else {
                    output.into()
                }
            )?;
        }
        Ok(())
    }

    pub fn render_tree(self) {
        for (r, row) in self.rows.iter().enumerate() {
            if r != self.rows.len() - 1 {
                print!("├── ");
            } else {
                print!("└── ");
            }
            for (i, cell) in row.iter().enumerate() {
                print!("{} ", cell.pad(self.widths[i]));
            }
            println!();
        }
    }
}

#[cfg(test)]
mod test {
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
        let mut s = Vec::new();
        let mut t = Table::new(TableOptions::default());

        t.push(["pineapple", "rosemary"]);
        t.push(["apples", "pears"]);
        t.write(&mut s).unwrap();

        #[rustfmt::skip]
        assert_eq!(
            String::from_utf8_lossy(&s),
            [
                "pineapple rosemary\n",
                "apples    pears\n"
            ].join("")
        );
    }

    #[test]
    fn test_table_truncate() {
        let mut s = Vec::new();
        let mut t = Table::new(TableOptions {
            max: Max {
                width: Some(16),
                height: None,
            },
            ..TableOptions::default()
        });

        t.push(["pineapple", "rosemary"]);
        t.push(["apples", "pears"]);
        t.write(&mut s).unwrap();

        #[rustfmt::skip]
        assert_eq!(
            String::from_utf8_lossy(&s),
            [
                "pineapple rosem…\n",
                "apples    pears\n"
            ].join("")
        );
    }

    #[test]
    fn test_table_unicode() {
        let mut s = Vec::new();
        let mut t = Table::new(TableOptions::default());

        t.push(["🍍pineapple", "__rosemary", "__sage"]);
        t.push(["__pears", "🍎apples", "🍌bananas"]);
        t.write(&mut s).unwrap();

        #[rustfmt::skip]
        assert_eq!(
            String::from_utf8_lossy(&s),
            [
                "🍍pineapple __rosemary __sage\n",
                "__pears     🍎apples   🍌bananas\n"
            ].join("")
        );
    }

    #[test]
    fn test_table_unicode_truncate() {
        let mut s = Vec::new();
        let mut t = Table::new(TableOptions {
            max: Max {
                width: Some(16),
                height: None,
            },
            ..TableOptions::default()
        });

        t.push(["🍍pineapple", "__rosemary"]);
        t.push(["__pears", "🍎apples"]);
        t.write(&mut s).unwrap();

        #[rustfmt::skip]
        assert_eq!(
            String::from_utf8_lossy(&s),
            [
                "🍍pineapple __r…\n",
                "__pears     🍎a…\n"
            ].join("")
        );
    }
}
