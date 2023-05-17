use radicle::cob::patch::{Patch, PatchId};
use radicle::Profile;

use radicle_cli::terminal::format;

use tuirealm::command::{Cmd, CmdResult};
use tuirealm::props::Color;
use tuirealm::tui::layout::Rect;
use tuirealm::{AttrValue, Attribute, Frame, MockComponent, Props, State};

use super::{Widget, WidgetComponent};

use super::common;
use super::common::container::Tabs;
use super::common::context::{ContextBar, Shortcuts};
use super::common::label::Label;

use crate::ui::theme::Theme;
use crate::ui::{cob, layout};

pub struct Activity {
    label: Widget<Label>,
    context: Widget<ContextBar>,
    shortcuts: Widget<Shortcuts>,
}

impl Activity {
    pub fn new(
        label: Widget<Label>,
        context: Widget<ContextBar>,
        shortcuts: Widget<Shortcuts>,
    ) -> Self {
        Self {
            label,
            context,
            shortcuts,
        }
    }
}

impl WidgetComponent for Activity {
    fn view(&mut self, _properties: &Props, frame: &mut Frame, area: Rect) {
        let label_w = self
            .label
            .query(Attribute::Width)
            .unwrap_or(AttrValue::Size(1))
            .unwrap_size();
        let context_h = self
            .context
            .query(Attribute::Height)
            .unwrap_or(AttrValue::Size(0))
            .unwrap_size();
        let shortcuts_h = self
            .shortcuts
            .query(Attribute::Height)
            .unwrap_or(AttrValue::Size(0))
            .unwrap_size();
        let layout = layout::root_component_with_context(area, context_h, shortcuts_h);

        self.label
            .view(frame, layout::centered_label(label_w, layout[0]));
        self.context.view(frame, layout[1]);
        self.shortcuts.view(frame, layout[2]);
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _properties: &Props, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

pub struct Files {
    label: Widget<Label>,
    context: Widget<ContextBar>,
    shortcuts: Widget<Shortcuts>,
}

impl Files {
    pub fn new(
        label: Widget<Label>,
        context: Widget<ContextBar>,
        shortcuts: Widget<Shortcuts>,
    ) -> Self {
        Self {
            label,
            context,
            shortcuts,
        }
    }
}

impl WidgetComponent for Files {
    fn view(&mut self, _properties: &Props, frame: &mut Frame, area: Rect) {
        let label_w = self
            .label
            .query(Attribute::Width)
            .unwrap_or(AttrValue::Size(1))
            .unwrap_size();
        let context_h = self
            .context
            .query(Attribute::Height)
            .unwrap_or(AttrValue::Size(0))
            .unwrap_size();
        let shortcuts_h = self
            .shortcuts
            .query(Attribute::Height)
            .unwrap_or(AttrValue::Size(0))
            .unwrap_size();
        let layout = layout::root_component_with_context(area, context_h, shortcuts_h);

        self.label
            .view(frame, layout::centered_label(label_w, layout[0]));
        self.context.view(frame, layout[1]);
        self.shortcuts.view(frame, layout[2]);
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
            common::reversable_label("activity").foreground(theme.colors.tabs_highlighted_fg),
            common::reversable_label("files").foreground(theme.colors.tabs_highlighted_fg),
        ],
    )
}

pub fn activity(theme: &Theme, patch: (PatchId, &Patch), profile: &Profile) -> Widget<Activity> {
    let (id, patch) = patch;
    let shortcuts = common::shortcuts(
        theme,
        vec![
            common::shortcut(theme, "esc", "back"),
            common::shortcut(theme, "tab", "section"),
            common::shortcut(theme, "q", "quit"),
        ],
    );
    let context = context(theme, (id, patch), profile);

    let not_implemented = common::label("not implemented").foreground(theme.colors.default_fg);
    let activity = Activity::new(not_implemented, context, shortcuts);

    Widget::new(activity)
}

pub fn files(theme: &Theme, patch: (PatchId, &Patch), profile: &Profile) -> Widget<Activity> {
    let (id, patch) = patch;
    let shortcuts = common::shortcuts(
        theme,
        vec![
            common::shortcut(theme, "esc", "back"),
            common::shortcut(theme, "tab", "section"),
            common::shortcut(theme, "q", "quit"),
        ],
    );
    let context = context(theme, (id, patch), profile);

    let not_implemented = common::label("not implemented").foreground(theme.colors.default_fg);
    let files = Activity::new(not_implemented, context, shortcuts);

    Widget::new(files)
}

pub fn context(theme: &Theme, patch: (PatchId, &Patch), profile: &Profile) -> Widget<ContextBar> {
    let (id, patch) = patch;
    let (_, rev) = patch.latest();
    let is_you = *patch.author().id() == profile.did();

    let id = format::cob(&id);
    let title = patch.title();
    let author = cob::format_author(patch, profile);
    let comments = rev.discussion().comments().count();

    let context = common::label(" patch ").background(theme.colors.context_badge_bg);
    let id = common::label(&format!(" {id} "))
        .foreground(theme.colors.context_id_fg)
        .background(theme.colors.context_id_bg);
    let title = common::label(&format!(" {title} "))
        .foreground(theme.colors.default_fg)
        .background(theme.colors.context_bg);
    let author = common::label(&format!(" {author} "))
        .foreground(theme.colors.context_id_author_fg)
        .background(theme.colors.context_bg);
    let comments = common::label(&format!(" {comments} "))
        .foreground(Color::Rgb(70, 70, 70))
        .background(theme.colors.context_light_bg);

    let context_bar = ContextBar::new(context, id, author, title, comments);

    Widget::new(context_bar).height(1)
}
