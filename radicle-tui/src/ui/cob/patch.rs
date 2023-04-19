use std::time::{Duration, SystemTime, UNIX_EPOCH};

use radicle::Profile;
use timeago;

use radicle::cob::patch::{Patch, PatchId};

use tuirealm::props::{Color, TextSpan};

use crate::ui::components::list::List;
use crate::ui::theme::Theme;

pub fn format_status(_patch: &Patch) -> String {
    String::from(" ⏺ ")
}

pub fn format_id(id: PatchId) -> String {
    id.to_string()[0..10].to_string()
}

pub fn format_title(patch: &Patch) -> String {
    patch.title().to_string()
}

pub fn format_author(patch: &Patch, profile: &Profile) -> String {
    let author_did = patch.author().id();
    let start = &author_did.to_human()[0..4];
    let end = &author_did.to_human()[43..47];

    if *author_did == profile.did() {
        format!("did:key:{start}...{end} (you)")
    } else {
        format!("did:key:{start}...{end}")
    }
}

pub fn format_tags(patch: &Patch) -> String {
    format!("{:?}", patch.tags().collect::<Vec<_>>())
}

pub fn format_timestamp(patch: &Patch) -> String {
    let fmt = timeago::Formatter::new();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    fmt.convert(Duration::from_secs(now - patch.timestamp().as_secs()))
}

pub fn format_comments(patch: &Patch) -> String {
    let count = match patch.latest() {
        Some((_, rev)) => rev.discussion().len(),
        None => 0,
    };
    format!("{count}")
}

impl List for (PatchId, Patch) {
    fn row(&self, theme: &Theme, profile: &Profile) -> Vec<TextSpan> {
        let (_, patch) = self;

        let status = format_status(patch);
        let status = TextSpan::from(status).fg(Color::Green);

        let title = format_title(patch);
        let title = TextSpan::from(title).fg(theme.colors.browser_patch_list_title);

        let author = format_author(patch, profile);
        let author = TextSpan::from(author).fg(theme.colors.browser_patch_list_author);

        let tags = format_tags(patch);
        let tags = TextSpan::from(tags).fg(theme.colors.browser_patch_list_tags);

        let comments = format_comments(patch);
        let comments = TextSpan::from(comments).fg(theme.colors.browser_patch_list_comments);

        let timestamp = format_timestamp(patch);
        let timestamp = TextSpan::from(timestamp).fg(theme.colors.browser_patch_list_timestamp);

        vec![status, title, author, tags, comments, timestamp]
    }
}
