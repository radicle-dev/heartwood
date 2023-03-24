use tuirealm::props::Color;

#[derive(Debug)]
pub struct Colors {
    pub default_fg: Color,
    pub tabs_fg: Color,
    pub tabs_highlighted_fg: Color,
    pub workspaces_info_fg: Color,
    pub labeled_container_bg: Color,
    pub item_list_highlighted_bg: Color,
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
    pub tab_divider: char,
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
            tabs_fg: Color::Rgb(100, 100, 100),
            tabs_highlighted_fg: Color::Rgb(38, 162, 105),
            workspaces_info_fg: Color::Rgb(220, 140, 40),
            labeled_container_bg: Color::Rgb(20, 20, 20),
            item_list_highlighted_bg: Color::Rgb(40, 40, 40),
            property_name_fg: Color::Rgb(85, 85, 255),
            property_divider_fg: Color::Rgb(10, 206, 209),
            shortcut_short_fg: Color::Rgb(100, 100, 100),
            shortcut_long_fg: Color::Rgb(70, 70, 70),
            shortcutbar_divider_fg: Color::Rgb(70, 70, 70),
        },
        icons: Icons {
            property_divider: '∙',
            shortcutbar_divider: '∙',
            tab_divider: '|',
            whitespace: ' ',
        },
    }
}
