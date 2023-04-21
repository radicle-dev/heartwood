use radicle::cob::patch::{Patch, PatchId};
use radicle::identity::{Id, Project};
use radicle::Profile;

use crate::ui::components::common::container::Tabs;
use crate::ui::components::common::Browser;
use crate::ui::components::home::{Dashboard, IssueBrowser};
use crate::ui::theme::Theme;

use super::{common, Widget};

pub fn navigation(theme: &Theme) -> Widget<Tabs> {
    common::tabs(
        theme,
        vec![
            common::reversable_label("dashboard").foreground(theme.colors.tabs_highlighted_fg),
            common::reversable_label("issues").foreground(theme.colors.tabs_highlighted_fg),
            common::reversable_label("patches").foreground(theme.colors.tabs_highlighted_fg),
        ],
    )
}

pub fn dashboard(theme: &Theme, id: &Id, project: &Project) -> Widget<Dashboard> {
    let about = common::labeled_container(
        theme,
        "about",
        common::property_list(
            theme,
            vec![
                common::property(theme, "id", &id.to_string()),
                common::property(theme, "name", project.name()),
                common::property(theme, "description", project.description()),
            ],
        )
        .to_boxed(),
    );
    let shortcuts = common::shortcuts(
        theme,
        vec![
            common::shortcut(theme, "tab", "section"),
            common::shortcut(theme, "q", "quit"),
        ],
    );
    let dashboard = Dashboard::new(about, shortcuts);

    Widget::new(dashboard)
}

pub fn patches(
    theme: &Theme,
    items: &[(PatchId, Patch)],
    profile: &Profile,
) -> Widget<Browser<(PatchId, Patch)>> {
    let shortcuts = common::shortcuts(
        theme,
        vec![
            common::shortcut(theme, "tab", "section"),
            common::shortcut(theme, "↑/↓", "navigate"),
            common::shortcut(theme, "enter", "show"),
            common::shortcut(theme, "q", "quit"),
        ],
    );

    let labels = vec!["", "title", "author", "time", "comments", "tags"];
    let widths = vec![3u16, 42, 15, 10, 5, 25];

    let table = common::table(theme, &labels, &widths, items, profile);
    let browser: Browser<(PatchId, Patch)> = Browser::new(table, shortcuts);

    Widget::new(browser)
}

pub fn issues(theme: &Theme) -> Widget<IssueBrowser> {
    let shortcuts = common::shortcuts(
        theme,
        vec![
            common::shortcut(theme, "tab", "section"),
            common::shortcut(theme, "q", "quit"),
        ],
    );

    let not_implemented = common::label("not implemented").foreground(theme.colors.default_fg);
    let browser = IssueBrowser::new(not_implemented, shortcuts);

    Widget::new(browser)
}
