//! A dead simple ANSI terminal color painting library.
//!
//! This library is a port of the `yansi` crate.
//! Copyright (c) 2017 Sergio Benitez
//!
mod color;
mod display;
mod paint;
mod style;
#[cfg(test)]
mod tests;
mod windows;

pub use color::Color;
pub use display::{display, display_with, Context, Display};
pub use paint::paint;
pub use paint::Filled;
pub use paint::Paint;
pub use style::Style;
