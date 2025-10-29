mod args;

use std::collections::BTreeSet;

use anyhow::{anyhow, Context};

use radicle::cob::identity::{self, IdentityMut, Revision, RevisionId};
use radicle::cob::Title;
use radicle::identity::doc::update;
use radicle::identity::{doc, Doc, Identity, RawDoc};
use radicle::node::device::Device;
use radicle::node::NodeId;
use radicle::storage::{ReadStorage as _, WriteRepository};
use radicle::{cob, crypto, Profile};
use radicle_surf::diff::Diff;
use radicle_term::Element;

use crate::git::unified_diff::Encode as _;
use crate::git::Rev;
use crate::terminal as term;
use crate::terminal::args::Error;
use crate::terminal::patch::Message;

pub use args::Args;
use args::Command;

pub fn run(args: Args, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let storage = &profile.storage;
    let rid = if let Some(rid) = args.repo {
        rid
    } else {
        let (_, rid) = radicle::rad::cwd()?;
        rid
    };
    let repo = storage
        .repository(rid)
        .context(anyhow!("repository `{rid}` not found in local storage"))?;
    let mut identity = Identity::load_mut(&repo)?;
    let current = identity.current().clone();

    let interactive = args.interactive();
    let command = args.command.unwrap_or(Command::List);

    match command {
        Command::Accept { revision } => {
            let revision = get(revision, &identity, &repo)?.clone();
            let id = revision.id;
            let signer = term::signer(&profile)?;

            if !revision.is_active() {
                anyhow::bail!("cannot vote on revision that is {}", revision.state);
            }

            if interactive.confirm(format!("Accept revision {}?", term::format::tertiary(id))) {
                identity.accept(&revision.id, &signer)?;

                if let Some(revision) = identity.revision(&id) {
                    // Update the canonical head to point to the latest accepted revision.
                    if revision.is_accepted() && revision.id == identity.current {
                        repo.set_identity_head_to(revision.id)?;
                    }
                    // TODO: Different output if canonical changed?

                    if !args.quiet {
                        term::success!("Revision {id} accepted");
                        print_meta(revision, &current, &profile)?;
                    }
                }
            }
        }
        Command::Reject { revision } => {
            let revision = get(revision, &identity, &repo)?.clone();
            let signer = term::signer(&profile)?;

            if !revision.is_active() {
                anyhow::bail!("cannot vote on revision that is {}", revision.state);
            }

            if interactive.confirm(format!(
                "Reject revision {}?",
                term::format::tertiary(revision.id)
            )) {
                identity.reject(revision.id, &signer)?;

                if !args.quiet {
                    term::success!("Revision {} rejected", revision.id);
                    print_meta(&revision, &current, &profile)?;
                }
            }
        }
        Command::Edit {
            revision,
            title,
            description,
        } => {
            let revision = get(revision, &identity, &repo)?.clone();
            let signer = term::signer(&profile)?;

            if !revision.is_active() {
                anyhow::bail!("revision can no longer be edited");
            }
            let Some((title, description)) = edit_title_description(title, description)? else {
                anyhow::bail!("revision title or description missing");
            };
            identity.edit(revision.id, title, description, &signer)?;

            if !args.quiet {
                term::success!("Revision {} edited", revision.id);
            }
        }
        Command::Update {
            title,
            description,
            delegate: delegates,
            rescind,
            threshold,
            visibility,
            allow,
            disallow,
            payload,
            edit,
        } => {
            let proposal = {
                let mut proposal = current.doc.clone().edit();
                let allow = allow.into_iter().collect::<BTreeSet<_>>();
                let disallow = disallow.into_iter().collect::<BTreeSet<_>>();

                proposal.threshold = threshold.unwrap_or(proposal.threshold);

                let proposal = match visibility {
                    Some(edit) => update::visibility(proposal, edit),
                    None => proposal,
                };
                let proposal = match update::privacy_allow_list(proposal, allow, disallow) {
                    Ok(proposal) => proposal,
                    Err(e) => match e {
                        update::error::PrivacyAllowList::Overlapping(overlap) =>                     anyhow::bail!("`--allow` and `--disallow` must not overlap: {overlap:?}"),
                        update::error::PrivacyAllowList::PublicVisibility =>                         return Err(Error::WithHint {
                            err:
                            anyhow!("`--allow` and `--disallow` should only be used for private repositories"),
                            hint: "use `--visibility private` to make the repository private, or perhaps you meant to use `--delegate`/`--rescind`",
                        }.into())
                    }
                };
                let threshold = proposal.threshold;
                let proposal = match update::delegates(proposal, delegates, rescind, &repo)? {
                    Ok(proposal) => proposal,
                    Err(errs) => {
                        term::error(format!("failed to verify delegates for {rid}"));
                        term::error(format!(
                            "the threshold of {threshold} delegates cannot be met.."
                        ));
                        for e in errs {
                            print_delegate_verification_error(&e);
                        }
                        anyhow::bail!("fatal: refusing to update identity document");
                    }
                };

                // TODO(erikli): whenever `clap` starts supporting custom value parsers
                // for a series of values, we can parse into `Payload` implicitly.
                let payloads = args::parse_many_upserts(&payload).collect::<Result<Vec<_>, _>>()?;

                update::payload(proposal, payloads)?
            };

            // If `--edit` is specified, the document can also be edited via a text edit.
            let proposal = if edit {
                match term::editor::Editor::comment()
                    .extension("json")
                    .initial(serde_json::to_string_pretty(&current.doc)?)?
                    .edit()?
                {
                    Some(proposal) => serde_json::from_str::<RawDoc>(&proposal)?,
                    None => {
                        term::print(term::format::italic(
                            "Nothing to do. The document is up to date. See `rad inspect --identity`.",
                        ));
                        return Ok(());
                    }
                }
            } else {
                proposal
            };

            let proposal = update::verify(proposal)?;
            if proposal == current.doc {
                if !args.quiet {
                    term::print(term::format::italic(
                        "Nothing to do. The document is up to date. See `rad inspect --identity`.",
                    ));
                }
                return Ok(());
            }
            let signer = term::signer(&profile)?;
            let revision = update(title, description, proposal, &mut identity, &signer)?;

            if revision.is_accepted() && revision.parent == Some(current.id) {
                // Update the canonical head to point to the latest accepted revision.
                repo.set_identity_head_to(revision.id)?;
            }
            if args.quiet {
                term::print(revision.id);
            } else {
                term::success!(
                    "Identity revision {} created",
                    term::format::tertiary(revision.id)
                );
                print(&revision, &current, &repo, &profile)?;
            }
        }
        Command::List => {
            let mut revisions =
                term::Table::<7, term::Label>::new(term::table::TableOptions::bordered());

            revisions.header([
                term::format::dim(String::from("●")).into(),
                term::format::bold(String::from("ID")).into(),
                term::format::bold(String::from("Title")).into(),
                term::format::bold(String::from("Author")).into(),
                term::Label::blank(),
                term::format::bold(String::from("Status")).into(),
                term::format::bold(String::from("Created")).into(),
            ]);
            revisions.divider();

            for r in identity.revisions().rev() {
                let icon = match r.state {
                    identity::State::Active => term::format::tertiary("●"),
                    identity::State::Accepted => term::format::positive("●"),
                    identity::State::Rejected => term::format::negative("●"),
                    identity::State::Stale => term::format::dim("●"),
                }
                .into();
                let state = r.state.to_string().into();
                let id = term::format::oid(r.id).into();
                let title = term::label(r.title.to_string());
                let (alias, author) =
                    term::format::Author::new(r.author.public_key(), &profile, true).labels();
                let timestamp = term::format::timestamp(r.timestamp).into();

                revisions.push([icon, id, title, alias, author, state, timestamp]);
            }
            revisions.print();
        }
        Command::Redact { revision } => {
            let revision = get(revision, &identity, &repo)?.clone();
            let signer = term::signer(&profile)?;

            if revision.is_accepted() {
                anyhow::bail!("cannot redact accepted revision");
            }
            if interactive.confirm(format!(
                "Redact revision {}?",
                term::format::tertiary(revision.id)
            )) {
                identity.redact(revision.id, &signer)?;

                if !args.quiet {
                    term::success!("Revision {} redacted", revision.id);
                }
            }
        }
        Command::Show { revision } => {
            let revision = get(revision, &identity, &repo)?;
            let previous = revision.parent.unwrap_or(revision.id);
            let previous = identity
                .revision(&previous)
                .ok_or(anyhow!("revision `{previous}` not found"))?;

            print(revision, previous, &repo, &profile)?;
        }
    }
    Ok(())
}

fn get<'a>(
    revision: Rev,
    identity: &'a Identity,
    repo: &radicle::storage::git::Repository,
) -> anyhow::Result<&'a Revision> {
    let id = revision.resolve(&repo.backend)?;
    let revision = identity
        .revision(&id)
        .ok_or(anyhow!("revision `{id}` not found"))?;

    Ok(revision)
}

fn print_meta(revision: &Revision, previous: &Doc, profile: &Profile) -> anyhow::Result<()> {
    let mut attrs = term::Table::<2, term::Label>::new(Default::default());

    attrs.push([
        term::format::bold("Title").into(),
        term::label(revision.title.to_string()),
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
        let author = term::format::Author::new(&id, profile, true);
        signatures.push([
            term::PREFIX_SUCCESS.into(),
            id.to_string().into(),
            author.alias().unwrap_or_default(),
            author.you().unwrap_or_default(),
        ]);
    }
    for id in rejected {
        let author = term::format::Author::new(&id, profile, true);
        signatures.push([
            term::PREFIX_ERROR.into(),
            id.to_string().into(),
            author.alias().unwrap_or_default(),
            author.you().unwrap_or_default(),
        ]);
    }
    for id in unknown {
        let author = term::format::Author::new(id, profile, true);
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

fn print(
    revision: &identity::Revision,
    previous: &identity::Revision,
    repo: &radicle::storage::git::Repository,
    profile: &Profile,
) -> anyhow::Result<()> {
    print_meta(revision, previous, profile)?;
    println!();
    print_diff(revision.parent.as_ref(), &revision.id, repo)?;

    Ok(())
}

fn edit_title_description(
    title: Option<Title>,
    description: Option<String>,
) -> anyhow::Result<Option<(Title, String)>> {
    const HELP: &str = r#"<!--
Please enter a patch message for your changes. An empty
message aborts the patch proposal.

The first line is the patch title. The patch description
follows, and must be separated with a blank line, just
like a commit message. Markdown is supported in the title
and description.
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

fn update<R, G>(
    title: Option<Title>,
    description: Option<String>,
    doc: Doc,
    current: &mut IdentityMut<R>,
    signer: &Device<G>,
) -> anyhow::Result<Revision>
where
    R: WriteRepository + cob::Store<Namespace = NodeId>,
    G: crypto::signature::Signer<crypto::Signature>,
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

fn print_diff(
    previous: Option<&RevisionId>,
    current: &RevisionId,
    repo: &radicle::storage::git::Repository,
) -> anyhow::Result<()> {
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
    let repo = radicle::git::raw::Repository::init_opts(
        tmp.path(),
        radicle::git::raw::RepositoryInitOptions::new()
            .external_template(false)
            .bare(true),
    )?;

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

fn print_delegate_verification_error(err: &update::error::DelegateVerification) {
    use update::error::DelegateVerification::*;
    match err {
        MissingDefaultBranch { branch, did } => term::error(format!(
            "missing {} for {} in local storage",
            term::format::secondary(branch),
            term::format::did(did)
        )),
        MissingDelegate { did } => {
            term::error(format!("the delegate {did} is missing"));
            term::hint(format!(
                "run `rad follow {did}` to follow this missing peer"
            ));
        }
    }
}
