use crate::Paint;

#[deprecated]
pub fn default<D: std::fmt::Display>(msg: D) -> Paint<D> {
    Paint::new(msg)
}

pub fn wrap<D: std::fmt::Display>(msg: D) -> Paint<D> {
    Paint::wrapping(msg)
}

pub fn negative<T: Into<Paint<U>>, U>(msg: T) -> Paint<U> {
    msg.into().fg(crate::Color::Red)
}

pub fn positive<T: Into<Paint<U>>, U>(msg: T) -> Paint<U> {
    msg.into().fg(crate::Color::Green)
}

pub fn primary<T: Into<Paint<U>>, U>(msg: T) -> Paint<U> {
    msg.into().fg(crate::Color::Magenta)
}

pub fn secondary<T: Into<Paint<U>>, U>(msg: T) -> Paint<U> {
    msg.into().fg(crate::Color::Blue)
}

pub fn tertiary<T: Into<Paint<U>>, U>(msg: T) -> Paint<U> {
    msg.into().fg(crate::Color::Cyan)
}

#[deprecated]
pub fn yellow<T: Into<Paint<U>>, U>(msg: T) -> Paint<U> {
    msg.into().fg(crate::Color::Yellow)
}

pub fn hint<T: Into<Paint<U>>, U>(msg: T) -> Paint<U> {
    msg.into().fg(crate::Color::Yellow)
}

pub fn faint<T: Into<Paint<U>>, U>(msg: T) -> Paint<U> {
    msg.into().fg(crate::Color::Fixed(236))
}

pub fn highlight<T: Into<Paint<U>>, U>(msg: T) -> Paint<U> {
    msg.into().fg(crate::Color::Green).bold()
}

pub fn badge_primary<D: std::fmt::Display>(input: D) -> Paint<String> {
    if Paint::is_enabled() {
        Paint::magenta(format!(" {input} ")).invert()
    } else {
        Paint::new(format!("❲{input}❳"))
    }
}

pub fn badge_yellow<D: std::fmt::Display>(input: D) -> Paint<String> {
    if Paint::is_enabled() {
        Paint::yellow(format!(" {input} ")).invert()
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

#[deprecated]
pub fn bold<T: Into<Paint<U>>, U>(msg: T) -> Paint<U> {
    msg.into().bold()
}

#[deprecated]
pub fn dim<T: Into<Paint<U>>, U>(msg: T) -> Paint<U> {
    msg.into().dim()
}

#[deprecated]
pub fn italic<T: Into<Paint<U>>, U>(msg: T) -> Paint<U> {
    msg.into().italic().dim()
}

pub fn notice(msg: String) -> Paint<String> {


        println!(
            "{} {}",
            self.display(&Paint::new("!").dim()),
            self.display(&msg)
        )
}