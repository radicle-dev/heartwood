use tuirealm::props::Color;

#[derive(Debug)]
pub struct Colors {
    pub shortcut_short_fg: Color,
    pub shortcut_long_fg: Color,
    pub shortcutbar_divider_fg: Color,
}

#[derive(Debug)]
pub struct Icons {
    pub whitespace: char,
    pub shortcutbar_divider: char,
}

/// The Radicle TUI theme. Can be defined in a JSON config file. e.g.:
///
/// {
///     "name": "Radicle Dark",
///     "colors": {
///         "foreground": "#ffffff",
///         "highlightedBackground": "#000000",
///     },
///     "icons": {
///         "workspaces.divider": "|",
///         "shortcuts.divider: "∙",
///     }
/// }
#[derive(Debug)]
pub struct Theme {
    pub name: String,
    pub colors: Colors,
    pub icons: Icons,
}

pub fn default_dark() -> Theme {
    Theme {
        name: String::from("Radicle Dark"),
        colors: Colors {
            shortcut_short_fg: Color::Rgb(100, 100, 100),
            shortcut_long_fg: Color::Rgb(70, 70, 70),
            shortcutbar_divider_fg: Color::Rgb(70, 70, 70),
        },
        icons: Icons {
            whitespace: ' ',
            shortcutbar_divider: '∙',
        },
    }
}
