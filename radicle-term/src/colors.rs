use crate::ansi::Color;

/// The faintest color; useful for borders and such.
pub const FAINT: Color = Color::Fixed(236);

/// Negative color, useful for errors.
pub const NEGATIVE: Color = Color::Red;
