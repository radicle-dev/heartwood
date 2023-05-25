use radicle::cob::patch::{Patch, PatchId};
use radicle::Profile;

use radicle::prelude::{Id, Project};
use tuirealm::command::{Cmd, CmdResult};
use tuirealm::tui::layout::Rect;
use tuirealm::{AttrValue, Attribute, Frame, MockComponent, Props, State};

use super::{Widget, WidgetComponent};

use super::common::container::{LabeledContainer, Tabs};
use super::common::context::Shortcuts;
use super::common::label::Label;
use super::common::{self, Browser};

use crate::ui::layout;
use crate::ui::theme::Theme;

pub struct Dashboard {
    about: Widget<LabeledContainer>,
    shortcuts: Widget<Shortcuts>,
}

impl Dashboard {
    pub fn new(about: Widget<LabeledContainer>, shortcuts: Widget<Shortcuts>) -> Self {
        Self { about, shortcuts }
    }
}

impl WidgetComponent for Dashboard {
    fn view(&mut self, _properties: &Props, frame: &mut Frame, area: Rect) {
        let shortcuts_h = self
            .shortcuts
            .query(Attribute::Height)
            .unwrap_or(AttrValue::Size(0))
            .unwrap_size();
        let layout = layout::root_component(area, shortcuts_h);

        self.about.view(frame, layout[0]);
        self.shortcuts.view(frame, layout[1]);
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _properties: &Props, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

pub struct IssueBrowser {
    label: Widget<Label>,
    shortcuts: Widget<Shortcuts>,
}

impl IssueBrowser {
    pub fn new(label: Widget<Label>, shortcuts: Widget<Shortcuts>) -> Self {
        Self { label, shortcuts }
    }
}

impl WidgetComponent for IssueBrowser {
    fn view(&mut self, _properties: &Props, frame: &mut Frame, area: Rect) {
        let label_w = self
            .label
            .query(Attribute::Width)
            .unwrap_or(AttrValue::Size(1))
            .unwrap_size();
        let shortcuts_h = self
            .shortcuts
            .query(Attribute::Height)
            .unwrap_or(AttrValue::Size(0))
            .unwrap_size();
        let layout = layout::root_component(area, shortcuts_h);

        self.label
            .view(frame, layout::centered_label(label_w, layout[0]));
        self.shortcuts.view(frame, layout[1])
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _properties: &Props, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

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
