use std::fmt::Display;

use super::Paint;

use unicode_width::UnicodeWidthStr;

/// Text that can be displayed on the terminal, measured, truncated and padded.
pub trait Cell: Display {
    /// Type after truncation.
    type Truncated: Cell;
    /// Type after padding.
    type Padded: Cell;

    /// Cell display width in number of terminal columns.
    fn width(&self) -> usize;
    /// Truncate cell if longer than given width. Shows the delimiter if truncated.
    fn truncate(&self, width: usize, delim: &str) -> Self::Truncated;
    /// Pad the cell so that it is the given width, while keeping the content left-aligned.
    fn pad(&self, width: usize) -> Self::Padded;
}

impl Cell for Paint<String> {
    type Truncated = Self;
    type Padded = Self;

    fn width(&self) -> usize {
        UnicodeWidthStr::width(self.content())
    }

    fn truncate(&self, width: usize, delim: &str) -> Self {
        Self {
            item: self.item.truncate(width, delim),
            style: self.style,
        }
    }

    fn pad(&self, width: usize) -> Self {
        Self {
            item: self.item.pad(width),
            style: self.style,
        }
    }
}

impl Cell for Paint<&str> {
    type Truncated = Paint<String>;
    type Padded = Paint<String>;

    fn width(&self) -> usize {
        Cell::width(self.item)
    }

    fn truncate(&self, width: usize, delim: &str) -> Paint<String> {
        Paint {
            item: self.item.truncate(width, delim),
            style: self.style,
        }
    }

    fn pad(&self, width: usize) -> Paint<String> {
        Paint {
            item: self.item.pad(width),
            style: self.style,
        }
    }
}

impl Cell for String {
    type Truncated = Self;
    type Padded = Self;

    fn width(&self) -> usize {
        Cell::width(self.as_str())
    }

    fn truncate(&self, width: usize, delim: &str) -> Self {
        self.as_str().truncate(width, delim)
    }

    fn pad(&self, width: usize) -> Self {
        self.as_str().pad(width)
    }
}

impl Cell for str {
    type Truncated = String;
    type Padded = String;

    fn width(&self) -> usize {
        UnicodeWidthStr::width(self)
    }

    fn truncate(&self, width: usize, delim: &str) -> String {
        use unicode_segmentation::UnicodeSegmentation as _;

        if width < Cell::width(self) {
            let d = Cell::width(delim);
            if width < d {
                // If we can't even fit the delimiter, just return an empty string.
                return String::new();
            }
            // Find the unicode byte boundary where the display width is the largest,
            // while being smaller than the given max width.
            let mut cols = 0; // Number of visual columns we need.
            let mut boundary = 0; // Boundary in bytes.
            for g in self.graphemes(true) {
                let c = Cell::width(g);
                if cols + c + d > width {
                    break;
                }
                boundary += g.len();
                cols += c;
            }
            format!("{}{delim}", &self[..boundary])
        } else {
            self.to_owned()
        }
    }

    fn pad(&self, max: usize) -> String {
        let width = Cell::width(self);

        if width < max {
            format!("{self}{}", " ".repeat(max - width))
        } else {
            self.to_owned()
        }
    }
}

impl<T: Cell + ?Sized> Cell for &T {
    type Truncated = T::Truncated;
    type Padded = T::Padded;

    fn width(&self) -> usize {
        T::width(self)
    }

    fn truncate(&self, width: usize, delim: &str) -> Self::Truncated {
        T::truncate(self, width, delim)
    }

    fn pad(&self, width: usize) -> Self::Padded {
        T::pad(self, width)
    }
}
