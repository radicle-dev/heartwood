use tuirealm::props::Color;

const COLOR_DEFAULT_FG: Color = Color::Rgb(200, 200, 200);
const COLOR_DEFAULT_DARK_FG: Color = Color::Rgb(150, 150, 150);
const COLOR_DEFAULT_DARK: Color = Color::Rgb(100, 100, 100);
const COLOR_DEFAULT_DARKER: Color = Color::Rgb(70, 70, 70);
const COLOR_DEFAULT_DARKEST: Color = Color::Rgb(40, 40, 40);
const COLOR_DEFAULT_FAINT: Color = Color::Rgb(20, 20, 20);

#[derive(Debug, Clone)]
pub struct Colors {
    pub default_fg: Color,
    pub tabs_highlighted_fg: Color,
    pub workspaces_info_fg: Color,
    pub labeled_container_bg: Color,
    pub item_list_highlighted_bg: Color,
    pub property_name_fg: Color,
    pub property_divider_fg: Color,
    pub shortcut_short_fg: Color,
    pub shortcut_long_fg: Color,
    pub shortcutbar_divider_fg: Color,
    pub browser_patch_list_title: Color,
    pub browser_patch_list_author: Color,
    pub browser_patch_list_tags: Color,
    pub browser_patch_list_comments: Color,
    pub browser_patch_list_timestamp: Color,
    pub context_bg: Color,
    pub context_light_bg: Color,
    pub context_badge_bg: Color,
    pub context_id_fg: Color,
    pub context_id_bg: Color,
    pub context_id_author_fg: Color,
}

#[derive(Debug, Clone)]
pub struct Icons {
    pub property_divider: char,
    pub shortcutbar_divider: char,
    pub tab_divider: char,
    pub tab_overline: char,
    pub whitespace: char,
}

#[derive(Debug, Clone)]
pub struct Tables {
    pub spacing: u16,
}

/// The Radicle TUI theme. Will be defined in a JSON config file in the
/// future. e.g.:
/// {
///     "name": "Default",
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
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub colors: Colors,
    pub icons: Icons,
    pub tables: Tables,
}

pub fn default_dark() -> Theme {
    Theme {
        name: String::from("Default"),
        colors: Colors {
            default_fg: COLOR_DEFAULT_FG,
            tabs_highlighted_fg: Color::Magenta,
            workspaces_info_fg: Color::Yellow,
            labeled_container_bg: COLOR_DEFAULT_FAINT,
            item_list_highlighted_bg: COLOR_DEFAULT_DARKER,
            property_name_fg: Color::Cyan,
            property_divider_fg: COLOR_DEFAULT_FG,
            shortcut_short_fg: COLOR_DEFAULT_DARK,
            shortcut_long_fg: COLOR_DEFAULT_DARKER,
            shortcutbar_divider_fg: COLOR_DEFAULT_DARKER,
            browser_patch_list_title: COLOR_DEFAULT_FG,
            browser_patch_list_author: Color::Gray,
            browser_patch_list_tags: Color::Yellow,
            browser_patch_list_comments: COLOR_DEFAULT_DARK_FG,
            browser_patch_list_timestamp: COLOR_DEFAULT_DARK,
            context_bg: COLOR_DEFAULT_DARKEST,
            context_light_bg: Color::Gray,
            context_badge_bg: Color::LightRed,
            context_id_fg: Color::Cyan,
            context_id_bg: COLOR_DEFAULT_DARKEST,
            context_id_author_fg: Color::Gray,
        },
        icons: Icons {
            property_divider: '∙',
            shortcutbar_divider: '∙',
            tab_divider: '|',
            tab_overline: '▔',
            whitespace: ' ',
        },
        tables: Tables { spacing: 2 },
    }
}
