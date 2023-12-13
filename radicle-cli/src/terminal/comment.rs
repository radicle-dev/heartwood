use radicle::cob::thread::{Comment, CommentId};
use radicle::Profile;

use crate::terminal as term;
use crate::terminal::format::Author;

/// Return a comment header as a [`term::Element`].
pub fn header<T>(
    id: &CommentId,
    comment: &Comment<T>,
    profile: &Profile,
) -> term::hstack::HStack<'static> {
    let author = comment.author();
    let author = Author::new(&author, profile);
    let (alias, nid) = author.labels();

    term::hstack::HStack::default()
        .child(term::Line::spaced([
            alias,
            nid,
            term::format::timestamp(comment.timestamp()).dim().into(),
        ]))
        .child(term::Line::new(term::Label::space()))
        .child(term::Line::spaced([term::format::oid(*id)
            .fg(term::Color::Cyan)
            .into()]))
}

/// Return a full comment widget as a [`term::Element`].
pub fn widget<'a, T>(id: &CommentId, comment: &Comment<T>, profile: &Profile) -> term::VStack<'a> {
    term::vstack::bordered(header(id, comment, profile))
        .child(term::textarea(comment.body()).wrap(60))
}
