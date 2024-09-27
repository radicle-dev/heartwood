use std::sync::Mutex;

use super::Color::*;
use super::Paint;
use crate::display;
use crate::Context;
use crate::Display;

/// Ensures tests are running serially.
static SERIAL: Mutex<()> = Mutex::new(());

#[test]
fn colors_enabled() {
    let _guard = SERIAL.lock();

    assert_eq!(
        display(&Paint::new("text/plain")).to_string(),
        "text/plain".to_string()
    );
    assert_eq!(
display(&Paint::red("hi")).to_string(),
        "\x1B[31mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::black("hi")).to_string(),
        "\x1B[30mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::yellow("hi").bold()).to_string(),
        "\x1B[1;33mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::new("hi").fg(Yellow).bold()).to_string(),
        "\x1B[1;33mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::blue("hi").underline()).to_string(),
        "\x1B[4;34mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::green("hi").bold().underline()).to_string(),
        "\x1B[1;4;32mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::green("hi").underline().bold()).to_string(),
        "\x1B[1;4;32mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::magenta("hi").bg(White)).to_string(),
        "\x1B[47;35mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::red("hi").bg(Blue).fg(Yellow)).to_string(),
        "\x1B[44;33mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::cyan("hi").bg(Blue).fg(Yellow)).to_string(),
        "\x1B[44;33mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::cyan("hi").bold().bg(White)).to_string(),
        "\x1B[1;47;36mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::cyan("hi").underline().bg(White)).to_string(),
        "\x1B[4;47;36mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::cyan("hi").bold().underline().bg(White)).to_string(),
        "\x1B[1;4;47;36mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::cyan("hi").underline().bold().bg(White)).to_string(),
        "\x1B[1;4;47;36mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::fixed(100, "hi")).to_string(),
        "\x1B[38;5;100mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::fixed(100, "hi").bg(Magenta)).to_string(),
        "\x1B[45;38;5;100mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::fixed(100, "hi").bg(Fixed(200))).to_string(),
        "\x1B[48;5;200;38;5;100mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::rgb(70, 130, 180, "hi")).to_string(),
        "\x1B[38;2;70;130;180mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::rgb(70, 130, 180, "hi").bg(Blue)).to_string(),
        "\x1B[44;38;2;70;130;180mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::blue("hi").bg(RGB(70, 130, 180))).to_string(),
        "\x1B[48;2;70;130;180;34mhi\x1B[0m".to_string()
    );
    assert_eq!(
        display(&Paint::rgb(70, 130, 180, "hi")
            .bg(RGB(5, 10, 15)))
            .to_string(),
        "\x1B[48;2;5;10;15;38;2;70;130;180mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::new("hi").bold()).to_string(),
        "\x1B[1mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::new("hi").underline()).to_string(),
        "\x1B[4mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::new("hi").bold().underline()).to_string(),
        "\x1B[1;4mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::new("hi").dim()).to_string(),
        "\x1B[2mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::new("hi").italic()).to_string(),
        "\x1B[3mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::new("hi").blink()).to_string(),
        "\x1B[5mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::new("hi").invert()).to_string(),
        "\x1B[7mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::new("hi").hidden()).to_string(),
        "\x1B[8mhi\x1B[0m".to_string()
    );
    assert_eq!(
display(&Paint::new("hi").strikethrough()).to_string(),
        "\x1B[9mhi\x1B[0m".to_string()
    );
}

#[test]
fn colors_disabled() {
    let _guard = SERIAL.lock();

    let ctx = Context {
        ansi: false,
    };

    assert_eq!(
        Paint::new("text/plain").display(&ctx).to_string(),
        "text/plain".to_string()
    );
    assert_eq!(Paint::red("hi").display(&ctx).to_string(), "hi".to_string());
    assert_eq!(Paint::black("hi").display(&ctx).to_string(), "hi".to_string());
    assert_eq!(Paint::yellow("hi").bold().display(&ctx).to_string(), "hi".to_string());
    assert_eq!(
        Paint::new("hi").fg(Yellow).bold().display(&ctx).to_string(),
        "hi".to_string()
    );
    assert_eq!(Paint::blue("hi").underline().display(&ctx).to_string(), "hi".to_string());
    assert_eq!(
        Paint::green("hi").bold().underline().display(&ctx).to_string(),
        "hi".to_string()
    );
    assert_eq!(
        Paint::green("hi").underline().bold().display(&ctx).to_string(),
        "hi".to_string()
    );
    assert_eq!(Paint::magenta("hi").bg(White).display(&ctx).to_string(), "hi".to_string());
    assert_eq!(
        Paint::red("hi").bg(Blue).fg(Yellow).display(&ctx).to_string(),
        "hi".to_string()
    );
    assert_eq!(
        Paint::cyan("hi").bg(Blue).fg(Yellow).display(&ctx).to_string(),
        "hi".to_string()
    );
    assert_eq!(
        Paint::cyan("hi").bold().bg(White).display(&ctx).to_string(),
        "hi".to_string()
    );
    assert_eq!(
        Paint::cyan("hi").underline().bg(White).display(&ctx).to_string(),
        "hi".to_string()
    );
    assert_eq!(
        Paint::cyan("hi").bold().underline().bg(White).display(&ctx).to_string(),
        "hi".to_string()
    );
    assert_eq!(
        Paint::cyan("hi").underline().bold().bg(White).display(&ctx).to_string(),
        "hi".to_string()
    );
    assert_eq!(Paint::fixed(100, "hi").display(&ctx).to_string(), "hi".to_string());
    assert_eq!(
        Paint::fixed(100, "hi").bg(Magenta).display(&ctx).to_string(),
        "hi".to_string()
    );
    assert_eq!(
        Paint::fixed(100, "hi").bg(Fixed(200)).display(&ctx).to_string(),
        "hi".to_string()
    );
    assert_eq!(Paint::rgb(70, 130, 180, "hi").display(&ctx).to_string(), "hi".to_string());
    assert_eq!(
        Paint::rgb(70, 130, 180, "hi").bg(Blue).display(&ctx).to_string(),
        "hi".to_string()
    );
    assert_eq!(
        Paint::blue("hi").bg(RGB(70, 130, 180)).display(&ctx).to_string(),
        "hi".to_string()
    );
    assert_eq!(
        Paint::blue("hi").bg(RGB(70, 130, 180)).wrap().display(&ctx).to_string(),
        "hi".to_string()
    );
    assert_eq!(
        Paint::rgb(70, 130, 180, "hi")
            .bg(RGB(5, 10, 15))
            .display(&ctx)
            .to_string(),
        "hi".to_string()
    );
    assert_eq!(Paint::new("hi").bold().display(&ctx).to_string(), "hi".to_string());
    assert_eq!(Paint::new("hi").underline().display(&ctx).to_string(), "hi".to_string());
    assert_eq!(
        Paint::new("hi").bold().underline().display(&ctx).to_string(),
        "hi".to_string()
    );
    assert_eq!(Paint::new("hi").dim().display(&ctx).to_string(), "hi".to_string());
    assert_eq!(Paint::new("hi").italic().display(&ctx).to_string(), "hi".to_string());
    assert_eq!(Paint::new("hi").blink().display(&ctx).to_string(), "hi".to_string());
    assert_eq!(Paint::new("hi").invert().display(&ctx).to_string(), "hi".to_string());
    assert_eq!(Paint::new("hi").hidden().display(&ctx).to_string(), "hi".to_string());
    assert_eq!(
        Paint::new("hi").strikethrough().display(&ctx).to_string(),
        "hi".to_string()
    );
    assert_eq!(
        Paint::new("hi").strikethrough().wrap().display(&ctx).to_string(),
        "hi".to_string()
    );
}

#[test]
fn wrapping() {
    let _guard = SERIAL.lock();
    let inner = || format!("{} b {}", display(&Paint::red("a")), display(&Paint::green("c")));
    let inner2 = || format!("0 {} 1", display(&Paint::magenta(&inner()).wrap()));

    assert_eq!(
display(&Paint::new("text/plain").wrap()).to_string(),
        "text/plain".to_string()
    );
assert_eq!(
    display(&Paint::new(&inner()).wrap()).to_string(), inner());
    assert_eq!(
display(&Paint::new(&inner()).wrap()).to_string(),
        "\u{1b}[31ma\u{1b}[0m b \u{1b}[32mc\u{1b}[0m".to_string()
    );
    assert_eq!(
display(&Paint::new(&inner()).fg(Blue).wrap()).to_string(),
        "\u{1b}[34m\u{1b}[31ma\u{1b}[0m\u{1b}[34m b \
            \u{1b}[32mc\u{1b}[0m\u{1b}[34m\u{1b}[0m"
            .to_string()
    );
assert_eq!(display(&Paint::new(&inner2()).wrap()).to_string(), inner2());
    assert_eq!(
display(&Paint::new(&inner2()).wrap()).to_string(),
        "0 \u{1b}[35m\u{1b}[31ma\u{1b}[0m\u{1b}[35m b \
            \u{1b}[32mc\u{1b}[0m\u{1b}[35m\u{1b}[0m 1"
            .to_string()
    );
    assert_eq!(
display(&Paint::new(&inner2()).fg(Blue).wrap()).to_string(),
        "\u{1b}[34m0 \u{1b}[35m\u{1b}[31ma\u{1b}[0m\u{1b}[34m\u{1b}[35m b \
            \u{1b}[32mc\u{1b}[0m\u{1b}[34m\u{1b}[35m\u{1b}[0m\u{1b}[34m 1\u{1b}[0m"
            .to_string()
    );
}
