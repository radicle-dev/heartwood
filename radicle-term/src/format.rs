use crate::Paint;

pub fn default<D: std::fmt::Display>(msg: D) -> Paint<D> {
    Paint::new(msg)
}

pub fn wrap<D: std::fmt::Display>(msg: D) -> Paint<D> {
    Paint::wrapping(msg)
}

pub fn negative<D: std::fmt::Display>(msg: D) -> Paint<D> {
    Paint::red(msg)
}

pub fn positive<D: std::fmt::Display>(msg: D) -> Paint<D> {
    Paint::green(msg)
}

pub fn primary<D: std::fmt::Display>(msg: D) -> Paint<D> {
    Paint::magenta(msg)
}

pub fn secondary<D: std::fmt::Display>(msg: D) -> Paint<D> {
    Paint::blue(msg)
}

pub fn tertiary<D: std::fmt::Display>(msg: D) -> Paint<D> {
    Paint::cyan(msg)
}

pub fn yellow<D: std::fmt::Display>(msg: D) -> Paint<D> {
    Paint::yellow(msg)
}

pub fn faint<D: std::fmt::Display>(msg: D) -> Paint<D> {
    Paint::fixed(236, msg)
}

pub fn highlight<D: std::fmt::Debug + std::fmt::Display>(input: D) -> Paint<D> {
    Paint::green(input).bold()
}

pub fn badge_primary<D: std::fmt::Display>(input: D) -> Paint<String> {
    if Paint::is_enabled() {
        Paint::magenta(format!(" {input} ")).invert()
    } else {
        Paint::new(format!("❲{input}❳"))
    }
}

pub fn badge_positive<D: std::fmt::Display>(input: D) -> Paint<String> {
    if Paint::is_enabled() {
        Paint::green(format!(" {input} ")).invert()
    } else {
        Paint::new(format!("❲{input}❳"))
    }
}

pub fn badge_negative<D: std::fmt::Display>(input: D) -> Paint<String> {
    if Paint::is_enabled() {
        Paint::red(format!(" {input} ")).invert()
    } else {
        Paint::new(format!("❲{input}❳"))
    }
}

pub fn badge_secondary<D: std::fmt::Display>(input: D) -> Paint<String> {
    if Paint::is_enabled() {
        Paint::blue(format!(" {input} ")).invert()
    } else {
        Paint::new(format!("❲{input}❳"))
    }
}

pub fn bold<D: std::fmt::Display>(input: D) -> Paint<D> {
    Paint::white(input).bold()
}

pub fn dim<D: std::fmt::Display>(input: D) -> Paint<D> {
    Paint::new(input).dim()
}

pub fn italic<D: std::fmt::Display>(input: D) -> Paint<D> {
    Paint::new(input).italic().dim()
}
