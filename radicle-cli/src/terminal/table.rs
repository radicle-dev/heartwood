use std::fmt::Write;

use crate::terminal as term;

#[derive(Debug, Default)]
pub struct TableOptions {
    pub overflow: bool,
}

#[derive(Debug)]
pub struct Table<const W: usize> {
    rows: Vec<[String; W]>,
    widths: [usize; W],
    opts: TableOptions,
}

impl<const W: usize> Table<W> {
    pub fn new(opts: TableOptions) -> Self {
        Self {
            rows: Vec::new(),
            widths: [0; W],
            opts,
        }
    }

    pub fn default() -> Self {
        Self {
            rows: Vec::new(),
            widths: [0; W],
            opts: TableOptions::default(),
        }
    }

    pub fn push(&mut self, row: [String; W]) {
        for (i, cell) in row.iter().enumerate() {
            self.widths[i] = self.widths[i].max(console::measure_text_width(cell));
        }
        self.rows.push(row);
    }

    pub fn render(self) {
        let width = term::width(); // Terminal width.

        for row in &self.rows {
            let mut output = String::new();
            let cells = row.len();

            for (i, cell) in row.iter().enumerate() {
                if i == cells - 1 || self.opts.overflow {
                    write!(output, "{}", cell).ok();
                } else {
                    write!(
                        output,
                        "{} ",
                        console::pad_str(cell, self.widths[i], console::Alignment::Left, None)
                    )
                    .ok();
                }
            }
            println!("{}", console::truncate_str(&output, width - 1, "…"));
        }
    }

    pub fn render_tree(self) {
        for (r, row) in self.rows.iter().enumerate() {
            if r != self.rows.len() - 1 {
                print!("├── ");
            } else {
                print!("└── ");
            }
            for (i, cell) in row.iter().enumerate() {
                print!(
                    "{} ",
                    console::pad_str(cell, self.widths[i], console::Alignment::Left, None)
                );
            }
            println!();
        }
    }
}
