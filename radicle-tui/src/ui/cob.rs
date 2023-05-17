use radicle_surf;

use cli::terminal::format;
use radicle_cli as cli;

use radicle::prelude::Did;
use radicle::storage::git::Repository;
use radicle::storage::{Oid, ReadRepository};
use radicle::Profile;

use radicle::cob::patch::{Patch, PatchId, State};
use radicle::cob::Timestamp;

use tuirealm::props::{Color, Style};
use tuirealm::tui::widgets::Cell;

use crate::ui::theme::Theme;
use crate::ui::widget::common::list::TableItem;

/// An author item that can be used in tables, list or trees.
///
/// Breaks up dependencies to [`Profile`] and [`Repository`] that
/// would be needed if [`Author`] would be used directly.
#[derive(Clone)]
pub struct AuthorItem {
    /// The author's DID.
    did: Did,
    /// True if the author is the current user.
    is_you: bool,
}

/// A patch item that can be used in tables, list or trees.
///
/// Breaks up dependencies to [`Profile`] and [`Repository`] that
/// would be needed if [`Patch`] would be used directly.
#[derive(Clone)]
pub struct PatchItem {
    /// Patch OID.
    id: PatchId,
    /// Patch state.
    state: State,
    /// Patch title.
    title: String,
    /// Author of the latest revision.
    author: AuthorItem,
    /// Head of the latest revision.
    head: Oid,
    /// Lines added by the latest revision.
    added: u16,
    /// Lines removed by the latest revision.
    removed: u16,
    /// Time when patch was opened.
    timestamp: Timestamp,
}

impl PatchItem {
    pub fn id(&self) -> PatchId {
        self.id
    }
}

impl TryFrom<(&Profile, &Repository, PatchId, Patch)> for PatchItem {
    type Error = anyhow::Error;

    fn try_from(value: (&Profile, &Repository, PatchId, Patch)) -> Result<Self, Self::Error> {
        let (profile, repo, id, patch) = value;
        let (_, rev) = patch.latest();
        let repo = radicle_surf::Repository::open(repo.path())?;
        let base = repo.commit(rev.base())?;
        let head = repo.commit(rev.head())?;
        let diff = repo.diff(base.id, head.id)?;

        Ok(PatchItem {
            id,
            state: patch.state().clone(),
            title: patch.title().into(),
            author: AuthorItem {
                did: patch.author().id,
                is_you: *patch.author().id == *profile.did(),
            },
            head: rev.head(),
            added: diff.stats().insertions as u16,
            removed: diff.stats().deletions as u16,
            timestamp: rev.timestamp(),
        })
    }
}

impl TableItem<8> for PatchItem {
    fn row(&self, theme: &Theme) -> [Cell; 8] {
        let (icon, color) = format_state(&self.state);
        let state = Cell::from(icon).style(Style::default().fg(color));

        let id = Cell::from(format::cob(&self.id))
            .style(Style::default().fg(theme.colors.browser_patch_list_id));

        let title = Cell::from(self.title.clone())
            .style(Style::default().fg(theme.colors.browser_patch_list_title));

        let author = Cell::from(format_author(&self.author.did, self.author.is_you))
            .style(Style::default().fg(theme.colors.browser_patch_list_author));

        let head = Cell::from(format::oid(self.head))
            .style(Style::default().fg(theme.colors.browser_patch_list_head));

        let added = Cell::from(format!("{}", self.added))
            .style(Style::default().fg(theme.colors.browser_patch_list_added));

        let removed = Cell::from(format!("{}", self.removed))
            .style(Style::default().fg(theme.colors.browser_patch_list_removed));

        let updated = Cell::from(format::timestamp(&self.timestamp).to_string())
            .style(Style::default().fg(theme.colors.browser_patch_list_timestamp));

        [state, id, title, author, head, added, removed, updated]
    }
}

impl TableItem<1> for () {
    fn row(&self, _theme: &Theme) -> [Cell; 1] {
        [Cell::default()]
    }
}

pub fn format_state(state: &State) -> (String, Color) {
    match state {
        State::Open { conflicts: _ } => (" ● ".into(), Color::Green),
        State::Archived => (" ● ".into(), Color::Yellow),
        State::Draft => (" ● ".into(), Color::Gray),
        State::Merged {
            revision: _,
            commit: _,
        } => (" ✔ ".into(), Color::Blue),
    }
}

pub fn format_author(did: &Did, is_you: bool) -> String {
    if is_you {
        format!("{} (you)", format::did(did))
    } else {
        format!("{}", format::did(did))
    }
}
