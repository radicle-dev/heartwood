use std::io;

use radicle_term::table::TableOptions;
use radicle_term::{Table, VStack};

use radicle::cob;
use radicle::cob::issue;
use radicle::cob::issue::CloseReason;
use radicle::Profile;

use crate::terminal as term;
use crate::terminal::format::Author;
use crate::terminal::Element;

use super::Context as _;

pub const OPEN_MSG: &str = r#"
<!--
Please enter an issue title and description.

The first line is the issue title. The issue description
follows, and must be separated by a blank line, just
like a commit message. Markdown is supported in the title
and description.
-->
"#;

/// Display format.
#[derive(Default, Debug, PartialEq, Eq)]
pub enum Format {
    #[default]
    Full,
    Header,
}

pub fn get_title_description(
    title: Option<String>,
    description: Option<String>,
) -> io::Result<Option<(String, String)>> {
    term::patch::Message::edit_title_description(title, description, OPEN_MSG)
}

pub fn show(
    issue: &issue::Issue,
    id: &cob::ObjectId,
    format: Format,
    profile: &Profile,
) -> anyhow::Result<()> {
    let term = profile.terminal();
    let labels: Vec<String> = issue.labels().cloned().map(|t| t.into()).collect();
    let assignees: Vec<String> = issue
        .assignees()
        .map(|a| term.display(&term::format::did(a)).to_string())
        .collect();
    let author = issue.author();
    let did = author.id();
    let author = Author::new(did, profile);

    let mut attrs = Table::<2, term::Line>::new(TableOptions {
        spacing: 2,
        ..TableOptions::default()
    });

    attrs.push([
        term::format::tertiary("Title".to_owned()).into(),
        term::format::bold(issue.title().to_owned()).into(),
    ]);

    attrs.push([
        term::format::tertiary("Issue".to_owned()).into(),
        term::format::bold(id.to_string()).into(),
    ]);

    attrs.push([
        term::format::tertiary("Author".to_owned()).into(),
        author.line(),
    ]);

    if !labels.is_empty() {
        attrs.push([
            term::format::tertiary("Labels".to_owned()).into(),
            term::format::secondary(labels.join(", ")).into(),
        ]);
    }

    if !assignees.is_empty() {
        attrs.push([
            term::format::tertiary("Assignees".to_owned()).into(),
            term::format::dim(assignees.join(", ")).into(),
        ]);
    }

    attrs.push([
        term::format::tertiary("Status".to_owned()).into(),
        match issue.state() {
            issue::State::Open => term::format::positive("open".to_owned()).into(),
            issue::State::Closed {
                reason: CloseReason::Solved,
            } => term::Line::spaced([
                term::format::negative("closed").into(),
                term::format::negative("(solved)").italic().dim().into(),
            ]),
            issue::State::Closed {
                reason: CloseReason::Other,
            } => term::Line::spaced([term::format::negative("closed").into()]),
        },
    ]);

    let description = issue.description();
    let mut widget = VStack::default()
        .border(Some(term::colors::FAINT))
        .child(attrs)
        .children(if !description.is_empty() {
            vec![
                term::Label::blank().boxed(),
                term::textarea(description.trim()).wrap(60).boxed(),
            ]
        } else {
            vec![]
        });

    if format == Format::Full {
        for (id, comment) in issue.replies() {
            let hstack = term::comment::header(id, comment, profile);

            widget = widget.divider();
            widget.push(hstack);
            widget.push(term::textarea(comment.body()).wrap(60));
        }
    }
    widget.print_to(&term);

    Ok(())
}
