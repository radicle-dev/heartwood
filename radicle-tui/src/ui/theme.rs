use tuirealm::props::Color;

#[derive(Debug)]
pub struct Colors {
    pub default_fg: Color,
    pub property_name_fg: Color,
    pub property_divider_fg: Color,
    pub shortcut_short_fg: Color,
    pub shortcut_long_fg: Color,
    pub shortcutbar_divider_fg: Color,
}

#[derive(Debug)]
pub struct Icons {
    pub property_divider: char,
    pub shortcutbar_divider: char,
    pub whitespace: char,
}

/// The Radicle TUI theme. Will be defined in a JSON config file in the
/// future. e.g.:
/// {
///     "name": "Radicle Dark",
///     "colors": {
///         "foreground": "#ffffff",
///         "propertyForeground": "#ffffff",
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
            default_fg: Color::Rgb(200, 200, 200),
            property_name_fg: Color::Rgb(85, 85, 255),
            property_divider_fg: Color::Rgb(10, 206, 209),
            shortcut_short_fg: Color::Rgb(100, 100, 100),
            shortcut_long_fg: Color::Rgb(70, 70, 70),
            shortcutbar_divider_fg: Color::Rgb(70, 70, 70),
        },
        icons: Icons {
            property_divider: '∙',
            shortcutbar_divider: '∙',
            whitespace: ' ',
        },
    }
}
