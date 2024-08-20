use anyhow::anyhow;

use radicle::cob;
use radicle::cob::identity::{IdentityMut, Revision, RevisionId};
use radicle::crypto::Signer;
use radicle::identity::{doc, Doc, RawDoc};
use radicle::storage::{ReadRepository, WriteRepository};
use radicle::Profile;
use radicle_surf::diff::Diff;
use radicle_term::Element as _;

use crate::git::unified_diff::Encode as _;
use crate::terminal as term;
use crate::terminal::patch::Message;

pub fn propose_changes<R>(
    profile: &Profile,
    repo: &R,
    proposal: RawDoc,
    current: &mut IdentityMut<R>,
    title: Option<String>,
    description: Option<String>,
) -> anyhow::Result<Option<Revision>>
where
    R: ReadRepository + WriteRepository + cob::Store,
{
    // Verify that the project payload can still be parsed into the `Project` type.
    if let Err(doc::PayloadError::Json(e)) = proposal.project() {
        anyhow::bail!("failed to verify `xyz.radicle.project`: {e}");
    }

    let proposal = proposal.verified()?;
    if proposal == current.doc {
        return Ok(None);
    }
    let signer = term::signer(profile)?;
    // N.b. get the parent OID before updating the identity
    let parent = current.current().id;
    let revision = update(title, description, proposal, current, &signer)?;

    if revision.is_accepted() && revision.parent == Some(parent) {
        // Update the canonical head to point to the latest accepted revision.
        repo.set_identity_head_to(revision.id)?;
    }
    Ok(Some(revision))
}

pub fn update<R, G>(
    title: Option<String>,
    description: Option<String>,
    doc: Doc,
    current: &mut IdentityMut<R>,
    signer: &G,
) -> anyhow::Result<Revision>
where
    R: WriteRepository + cob::Store,
    G: Signer,
{
    if let Some((title, description)) = edit_title_description(title, description)? {
        let id = current.update(title, description, &doc, signer)?;
        let revision = current
            .revision(&id)
            .ok_or(anyhow!("update failed: revision {id} is missing"))?;

        Ok(revision.clone())
    } else {
        Err(anyhow!("you must provide a revision title and description"))
    }
}

pub fn edit_title_description(
    title: Option<String>,
    description: Option<String>,
) -> anyhow::Result<Option<(String, String)>> {
    const HELP: &str = r#"<!--
Please enter a message for your changes. An empty message aborts the proposal.

The first line is the title. The description follows, and must be separated with
a blank line, just like a commit message. Markdown is supported in the title and
description.
-->"#;

    let result = if let (Some(t), d) = (title.as_ref(), description.as_deref()) {
        Some((t.to_owned(), d.unwrap_or_default().to_owned()))
    } else {
        let result = Message::edit_title_description(title, description, HELP)?;
        if let Some((title, description)) = result {
            Some((title, description))
        } else {
            None
        }
    };
    Ok(result)
}

pub fn print<R>(
    revision: &Revision,
    previous: &Revision,
    repo: &R,
    profile: &Profile,
) -> anyhow::Result<()>
where
    R: ReadRepository,
{
    print_meta(revision, previous, profile)?;
    println!();
    print_diff(revision.parent.as_ref(), &revision.id, repo)?;

    Ok(())
}

fn print_meta(revision: &Revision, previous: &Doc, profile: &Profile) -> anyhow::Result<()> {
    let mut attrs = term::Table::<2, term::Label>::new(Default::default());

    attrs.push([
        term::format::bold("Title").into(),
        term::label(revision.title.to_owned()),
    ]);
    attrs.push([
        term::format::bold("Revision").into(),
        term::label(revision.id.to_string()),
    ]);
    attrs.push([
        term::format::bold("Blob").into(),
        term::label(revision.blob.to_string()),
    ]);
    attrs.push([
        term::format::bold("Author").into(),
        term::label(revision.author.to_string()),
    ]);
    attrs.push([
        term::format::bold("State").into(),
        term::label(revision.state.to_string()),
    ]);
    attrs.push([
        term::format::bold("Quorum").into(),
        if revision.is_accepted() {
            term::format::positive("yes").into()
        } else {
            term::format::negative("no").into()
        },
    ]);

    let mut meta = term::VStack::default()
        .border(Some(term::colors::FAINT))
        .child(attrs)
        .children(if !revision.description.is_empty() {
            vec![
                term::Label::blank().boxed(),
                term::textarea(revision.description.to_owned()).boxed(),
            ]
        } else {
            vec![]
        })
        .divider();

    let accepted = revision.accepted().collect::<Vec<_>>();
    let rejected = revision.rejected().collect::<Vec<_>>();
    let unknown = previous
        .delegates()
        .iter()
        .filter(|id| !accepted.contains(id) && !rejected.contains(id))
        .collect::<Vec<_>>();
    let mut signatures = term::Table::<4, _>::default();

    for id in accepted {
        let author = term::format::Author::new(&id, profile);
        signatures.push([
            term::format::positive("✓").into(),
            id.to_string().into(),
            author.alias().unwrap_or_default(),
            author.you().unwrap_or_default(),
        ]);
    }
    for id in rejected {
        let author = term::format::Author::new(&id, profile);
        signatures.push([
            term::format::negative("✗").into(),
            id.to_string().into(),
            author.alias().unwrap_or_default(),
            author.you().unwrap_or_default(),
        ]);
    }
    for id in unknown {
        let author = term::format::Author::new(id, profile);
        signatures.push([
            term::format::dim("?").into(),
            id.to_string().into(),
            author.alias().unwrap_or_default(),
            author.you().unwrap_or_default(),
        ]);
    }
    meta.push(signatures);
    meta.print();

    Ok(())
}

fn print_diff<R>(
    previous: Option<&RevisionId>,
    current: &RevisionId,
    repo: &R,
) -> anyhow::Result<()>
where
    R: ReadRepository,
{
    let previous = if let Some(previous) = previous {
        let previous = Doc::load_at(*previous, repo)?;
        let previous = serde_json::to_string_pretty(&previous.doc)?;

        Some(previous)
    } else {
        None
    };
    let current = Doc::load_at(*current, repo)?;
    let current = serde_json::to_string_pretty(&current.doc)?;

    let tmp = tempfile::tempdir()?;
    let repo = radicle::git::raw::Repository::init_bare(tmp.path())?;

    let previous = if let Some(previous) = previous {
        let tree = radicle::git::write_tree(&doc::PATH, previous.as_bytes(), &repo)?;
        Some(tree)
    } else {
        None
    };
    let current = radicle::git::write_tree(&doc::PATH, current.as_bytes(), &repo)?;
    let mut opts = radicle::git::raw::DiffOptions::new();
    opts.context_lines(u32::MAX);

    let diff = repo.diff_tree_to_tree(previous.as_ref(), Some(&current), Some(&mut opts))?;
    let diff = Diff::try_from(diff)?;

    if let Some(modified) = diff.modified().next() {
        let diff = modified.diff.to_unified_string()?;
        print!("{diff}");
    } else {
        term::print(term::format::italic("No changes."));
    }
    Ok(())
}
