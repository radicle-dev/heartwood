use radicle::Profile;
use tuirealm::props::Color;

use radicle::cob::patch::{Patch, PatchId};

use super::common;
use super::Widget;

use crate::ui::cob::patch;
use crate::ui::components::common::container::Tabs;
use crate::ui::components::common::context::ContextBar;
use crate::ui::components::patch::Activity;
use crate::ui::theme::Theme;

pub fn navigation(theme: &Theme) -> Widget<Tabs> {
    common::tabs(
        theme,
        vec![common::label("activity"), common::label("files")],
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

pub fn context(_theme: &Theme, patch: (PatchId, &Patch), profile: &Profile) -> Widget<ContextBar> {
    let (id, patch) = patch;
    let id = patch::format_id(id);
    let title = patch.title();
    let author = patch::format_author(patch, profile);
    let comments = patch::format_comments(patch);

    let context = common::label(" patch ").background(Color::Rgb(238, 111, 248));
    let id = common::label(&format!(" {id} "))
        .foreground(Color::Rgb(117, 113, 249))
        .background(Color::Rgb(40, 40, 40));
    let title = common::label(&format!(" {title} "))
        .foreground(Color::Rgb(70, 70, 70))
        .background(Color::Rgb(40, 40, 40));
    let author = common::label(&format!(" {author} "))
        .foreground(Color::Rgb(117, 113, 249))
        .background(Color::Rgb(40, 40, 40));
    let comments = common::label(&format!(" {comments} "))
        .foreground(Color::Rgb(70, 70, 70))
        .background(Color::Rgb(50, 50, 50));

    let context_bar = ContextBar::new(context, id, author, title, comments);

    Widget::new(context_bar).height(1)
}
