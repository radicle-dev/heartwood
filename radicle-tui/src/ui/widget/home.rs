use radicle::cob::patch::{Patch, PatchId};
use radicle::identity::{Id, Project};
use radicle::Profile;
use tuirealm::props::{PropPayload, PropValue, TextSpan};
use tuirealm::AttrValue;

use crate::ui::components::container::Tabs;
use crate::ui::components::workspace::{Browser, Dashboard, IssueBrowser};
use crate::ui::theme::Theme;

use super::{common, Widget};

pub fn navigation(theme: &Theme) -> Widget<Tabs> {
    common::tabs(
        theme,
        vec![
            common::label("dashboard"),
            common::label("issues"),
            common::label("patches"),
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

    let widths = AttrValue::Payload(PropPayload::Vec(vec![
        PropValue::U16(2),
        PropValue::U16(43),
        PropValue::U16(15),
        PropValue::U16(15),
        PropValue::U16(5),
        PropValue::U16(20),
    ]));
    let header = AttrValue::Payload(PropPayload::Vec(vec![
        PropValue::TextSpan(TextSpan::from("")),
        PropValue::TextSpan(TextSpan::from("title")),
        PropValue::TextSpan(TextSpan::from("author")),
        PropValue::TextSpan(TextSpan::from("tags")),
        PropValue::TextSpan(TextSpan::from("comments")),
        PropValue::TextSpan(TextSpan::from("date")),
    ]));

    let table = common::table(theme, items, profile)
        .custom("widths", widths)
        .custom("header", header);
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
