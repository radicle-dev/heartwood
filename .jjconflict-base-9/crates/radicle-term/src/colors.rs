use crate::ansi::Color;

/// The faintest color; useful for borders and such.
pub const FAINT: Color = fixed::FAINT;

// RGB (24-bit) colors supported by modern terminals.
pub mod rgb {
    use super::*;

    pub const NEGATIVE: Color = Color::RGB(60, 10, 20);
    pub const POSITIVE: Color = Color::RGB(10, 60, 20);
    pub const NEGATIVE_DARK: Color = Color::RGB(30, 10, 20);
    pub const POSITIVE_DARK: Color = Color::RGB(10, 30, 20);
    pub const NEGATIVE_LIGHT: Color = Color::RGB(170, 80, 120);
    pub const POSITIVE_LIGHT: Color = Color::RGB(80, 170, 120);
    pub const DIM: Color = Color::RGB(100, 100, 100);
    pub const FAINT: Color = Color::RGB(20, 20, 20);

    // Default syntax theme.
    pub const PURPLE: Color = Color::RGB(0xd2, 0xa8, 0xff);
    pub const RED: Color = Color::RGB(0xff, 0x7b, 0x72);
    pub const GREEN: Color = Color::RGB(0x7e, 0xd7, 0x87);
    pub const TEAL: Color = Color::RGB(0xa5, 0xd6, 0xff);
    pub const ORANGE: Color = Color::RGB(0xff, 0xa6, 0x57);
    pub const BLUE: Color = Color::RGB(0x79, 0xc0, 0xff);
    pub const GREY: Color = Color::RGB(0x8b, 0x94, 0x9e);
    pub const GREY_LIGHT: Color = Color::RGB(0xc9, 0xd1, 0xd9);

    /// Get a color using the color name.
    pub fn theme(name: &'static str) -> Option<Color> {
        match name {
            "negative" => Some(NEGATIVE),
            "negative.dark" => Some(NEGATIVE_DARK),
            "negative.light" => Some(NEGATIVE_LIGHT),
            "positive" => Some(POSITIVE),
            "positive.dark" => Some(POSITIVE_DARK),
            "positive.light" => Some(POSITIVE_LIGHT),
            "dim" => Some(DIM),
            "faint" => Some(FAINT),
            "purple" => Some(PURPLE),
            "red" => Some(RED),
            "green" => Some(GREEN),
            "teal" => Some(TEAL),
            "orange" => Some(ORANGE),
            "blue" => Some(BLUE),
            "grey" => Some(GREY),
            "grey.light" => Some(GREY_LIGHT),

            _ => None,
        }
    }
}

/// "Fixed" ANSI colors, supported by most terminals.
pub mod fixed {
    use super::*;

    /// The faintest color; useful for borders and such.
    pub const FAINT: Color = Color::Fixed(236);
    /// Slightly brighter than faint.
    pub const DIM: Color = Color::Fixed(239);

    /// Get a color using the color name.
    pub fn theme(name: &'static str) -> Option<Color> {
        match name {
            "negative" => Some(Color::Red),
            "negative.dark" => None,
            "positive" => Some(Color::Green),
            "positive.dark" => None,
            "dim" => None,
            "faint" => None,
            "blue" => Some(Color::Blue),
            "green" => Some(Color::Green),
            "red" => Some(Color::Red),
            "teal" => Some(Color::Cyan),
            "purple" => Some(Color::Magenta),

            _ => None,
        }
    }
}
