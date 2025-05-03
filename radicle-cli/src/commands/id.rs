use std::collections::BTreeSet;
use std::str::FromStr;
use std::{ffi::OsString, io};

use anyhow::{anyhow, Context};

use radicle::cob::identity;
use radicle::cob::identity::Revision;
use radicle::identity::{doc, Doc, Identity, RawDoc, Visibility};
use radicle::prelude::{Did, RepoId};
use radicle::storage::{refs, HasRepoId};
use radicle::storage::{ReadRepository, ReadStorage as _, WriteRepository};
use radicle::Profile;
use radicle_term::Element;
use serde_json as json;

use crate::git::Rev;
use crate::id;
use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::Interactive;

pub const HELP: Help = Help {
    name: "id",
    description: "Manage repository identities",
    version: env!("RADICLE_VERSION"),
    usage: r#"
Usage

    rad id list [<option>...]
    rad id update [--title <string>] [--description <string>]
                  [--delegate <did>] [--rescind <did>]
                  [--visibility <private | public>]
                  [--allow <did>] [--disallow <did>]
                  [--no-confirm] [--payload <id> <key> <val>...] [--edit] [<option>...]
    rad id edit <revision-id> [--title <string>] [--description <string>] [<option>...]
    rad id show <revision-id> [<option>...]
    rad id <accept | reject | redact> <revision-id> [<option>...]
    rad id migrate [--no-confirm] [--title <string>] [--description <string>]

    The *rad id* command is used to manage and propose changes to the
    identity of a Radicle repository.

    See the rad-id(1) man page for more information.

Options

    --repo <rid>           Repository (defaults to the current repository)
    --quiet, -q            Don't print anything
    --help                 Print help
"#,
};

#[derive(Clone, Debug, Default)]
pub enum Operation {
    Migrate {
        title: Option<String>,
        description: Option<String>,
    },
    Update {
        title: Option<String>,
        description: Option<String>,
        delegate: Vec<Did>,
        rescind: Vec<Did>,
        visibility: Option<EditVisibility>,
        allow: BTreeSet<Did>,
        disallow: BTreeSet<Did>,
        payload: Vec<(doc::PayloadId, String, json::Value)>,
        edit: bool,
    },
    AcceptRevision {
        revision: Rev,
    },
    RejectRevision {
        revision: Rev,
    },
    EditRevision {
        revision: Rev,
        title: Option<String>,
        description: Option<String>,
    },
    RedactRevision {
        revision: Rev,
    },
    ShowRevision {
        revision: Rev,
    },
    #[default]
    ListRevisions,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EditVisibility {
    #[default]
    Public,
    Private,
}

#[derive(thiserror::Error, Debug)]
#[error("'{0}' is not a valid visibility type")]
pub struct EditVisibilityParseError(String);

impl FromStr for EditVisibility {
    type Err = EditVisibilityParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "public" => Ok(EditVisibility::Public),
            "private" => Ok(EditVisibility::Private),
            _ => Err(EditVisibilityParseError(s.to_owned())),
        }
    }
}

#[derive(Default, PartialEq, Eq)]
pub enum OperationName {
    Accept,
    Reject,
    Edit,
    Update,
    Migrate,
    Show,
    Redact,
    #[default]
    List,
}

pub struct Options {
    pub op: Operation,
    pub rid: Option<RepoId>,
    pub interactive: Interactive,
    pub quiet: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut op: Option<OperationName> = None;
        let mut revision: Option<Rev> = None;
        let mut rid: Option<RepoId> = None;
        let mut title: Option<String> = None;
        let mut description: Option<String> = None;
        let mut delegate: Vec<Did> = Vec::new();
        let mut rescind: Vec<Did> = Vec::new();
        let mut visibility: Option<EditVisibility> = None;
        let mut allow: BTreeSet<Did> = BTreeSet::new();
        let mut disallow: BTreeSet<Did> = BTreeSet::new();
        let mut interactive = Interactive::new(io::stdout());
        let mut payload = Vec::new();
        let mut edit = false;
        let mut quiet = false;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") => {
                    return Err(Error::HelpManual { name: "rad-id" }.into());
                }
                Short('h') => {
                    return Err(Error::Help.into());
                }
                Long("title")
                    if op == Some(OperationName::Edit) || op == Some(OperationName::Update) =>
                {
                    title = Some(parser.value()?.to_string_lossy().into());
                }
                Long("description")
                    if op == Some(OperationName::Edit) || op == Some(OperationName::Update) =>
                {
                    description = Some(parser.value()?.to_string_lossy().into());
                }
                Long("quiet") | Short('q') => {
                    quiet = true;
                }
                Long("no-confirm") => {
                    interactive = Interactive::No;
                }
                Value(val) if op.is_none() => match val.to_string_lossy().as_ref() {
                    "e" | "edit" => op = Some(OperationName::Edit),
                    "u" | "update" => op = Some(OperationName::Update),
                    "m" | "migrate" => op = Some(OperationName::Migrate),
                    "l" | "list" => op = Some(OperationName::List),
                    "s" | "show" => op = Some(OperationName::Show),
                    "a" | "accept" => op = Some(OperationName::Accept),
                    "r" | "reject" => op = Some(OperationName::Reject),
                    "d" | "redact" => op = Some(OperationName::Redact),

                    unknown => anyhow::bail!("unknown operation '{}'", unknown),
                },
                Long("repo") => {
                    let val = parser.value()?;
                    let val = term::args::rid(&val)?;

                    rid = Some(val);
                }
                Long("delegate") => {
                    let did = term::args::did(&parser.value()?)?;
                    delegate.push(did);
                }
                Long("rescind") => {
                    let did = term::args::did(&parser.value()?)?;
                    rescind.push(did);
                }
                Long("allow") => {
                    let value = parser.value()?;
                    let did = term::args::did(&value)?;
                    allow.insert(did);
                }
                Long("disallow") => {
                    let value = parser.value()?;
                    let did = term::args::did(&value)?;
                    disallow.insert(did);
                }
                Long("visibility") => {
                    let value = parser.value()?;
                    let value = term::args::parse_value("visibility", value)?;

                    visibility = Some(value);
                }
                Long("payload") => {
                    let mut values = parser.values()?;
                    let id = values
                        .next()
                        .ok_or(anyhow!("expected payload id, eg. `xyz.radicle.project`"))?;
                    let id: doc::PayloadId = term::args::parse_value("payload", id)?;

                    let key = values
                        .next()
                        .ok_or(anyhow!("expected payload key, eg. 'defaultBranch'"))?;
                    let key = term::args::string(&key);

                    let val = values
                        .next()
                        .ok_or(anyhow!("expected payload value, eg. '\"heartwood\"'"))?;
                    let val = val.to_string_lossy().to_string();
                    let val = json::from_str(val.as_str())
                        .map_err(|e| anyhow!("invalid JSON value `{val}`: {e}"))?;

                    payload.push((id, key, val));
                }
                Long("edit") => {
                    edit = true;
                }
                Value(val) => {
                    let val = term::args::rev(&val)?;
                    revision = Some(val);
                }
                _ => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        let op = match op.unwrap_or_default() {
            OperationName::Accept => Operation::AcceptRevision {
                revision: revision.ok_or_else(|| anyhow!("a revision must be provided"))?,
            },
            OperationName::Reject => Operation::RejectRevision {
                revision: revision.ok_or_else(|| anyhow!("a revision must be provided"))?,
            },
            OperationName::Edit => Operation::EditRevision {
                title,
                description,
                revision: revision.ok_or_else(|| anyhow!("a revision must be provided"))?,
            },
            OperationName::Show => Operation::ShowRevision {
                revision: revision.ok_or_else(|| anyhow!("a revision must be provided"))?,
            },
            OperationName::List => Operation::ListRevisions,
            OperationName::Redact => Operation::RedactRevision {
                revision: revision.ok_or_else(|| anyhow!("a revision must be provided"))?,
            },
            OperationName::Update => Operation::Update {
                title,
                description,
                delegate,
                rescind,
                visibility,
                allow,
                disallow,
                payload,
                edit,
            },
            OperationName::Migrate => Operation::Migrate { title, description },
        };
        Ok((
            Options {
                rid,
                op,
                interactive,
                quiet,
            },
            vec![],
        ))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let storage = &profile.storage;
    let rid = if let Some(rid) = options.rid {
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

    match options.op {
        Operation::AcceptRevision { revision } => {
            let revision = get(revision, &identity, &repo)?.clone();
            let id = revision.id;
            let signer = term::signer(&profile)?;

            if !revision.is_active() {
                anyhow::bail!("cannot vote on revision that is {}", revision.state);
            }

            if options
                .interactive
                .confirm(format!("Accept revision {}?", term::format::tertiary(id)))
            {
                identity.accept(&revision.id, &signer)?;

                if let Some(revision) = identity.revision(&id) {
                    // Update the canonical head to point to the latest accepted revision.
                    if revision.is_accepted() && revision.id == identity.current {
                        repo.set_identity_head_to(revision.id)?;
                    }
                    // TODO: Different output if canonical changed?

                    if !options.quiet {
                        term::success!("Revision {id} accepted");
                        print_meta(revision, &current, &profile)?;
                    }
                }
            }
        }
        Operation::RejectRevision { revision } => {
            let revision = get(revision, &identity, &repo)?.clone();
            let signer = term::signer(&profile)?;

            if !revision.is_active() {
                anyhow::bail!("cannot vote on revision that is {}", revision.state);
            }

            if options.interactive.confirm(format!(
                "Reject revision {}?",
                term::format::tertiary(revision.id)
            )) {
                identity.reject(revision.id, &signer)?;

                if !options.quiet {
                    term::success!("Revision {} rejected", revision.id);
                    print_meta(&revision, &current, &profile)?;
                }
            }
        }
        Operation::EditRevision {
            revision,
            title,
            description,
        } => {
            let revision = get(revision, &identity, &repo)?.clone();
            let signer = term::signer(&profile)?;

            if !revision.is_active() {
                anyhow::bail!("revision can no longer be edited");
            }
            let Some((title, description)) = id::edit_title_description(title, description)? else {
                anyhow::bail!("revision title or description missing");
            };
            identity.edit(revision.id, title, description, &signer)?;

            if !options.quiet {
                term::success!("Revision {} edited", revision.id);
            }
        }
        Operation::Update {
            title,
            description,
            delegate: delegates,
            rescind,
            visibility,
            allow,
            disallow,
            payload,
            edit,
        } => {
            let proposal = {
                let mut proposal = current.doc.clone().edit();

                if !allow.is_disjoint(&disallow) {
                    let overlap = allow
                        .intersection(&disallow)
                        .map(Did::to_string)
                        .collect::<Vec<_>>();
                    anyhow::bail!("`--allow` and `--disallow` must not overlap: {overlap:?}")
                }

                match (&mut proposal.visibility, visibility) {
                    (Visibility::Public, None | Some(EditVisibility::Public)) if !allow.is_empty() || !disallow.is_empty() => {
                        return Err(Error::WithHint {
                            err:
                            anyhow!("`--allow` and `--disallow` should only be used for private repositories"),
                            hint: "use `--visibility private` to make the repository private, or perhaps you meant to use `--delegate`/`--rescind`",
                        }.into())
                    }
                    (Visibility::Public, None | Some(EditVisibility::Public)) => { /* no-op */ },
                    (Visibility::Private { allow: existing }, None | Some(EditVisibility::Private)) => {
                        for did in allow {
                            existing.insert(did);
                        }
                        for did in disallow {
                            existing.remove(&did);
                        }
                    }
                    (Visibility::Public, Some(EditVisibility::Private)) => {
                        // We ignore disallow since only allowing matters and the sets are disjoint.
                        proposal.visibility = Visibility::Private { allow };
                    }
                    (Visibility::Private { .. }, Some(EditVisibility::Public)) if !allow.is_empty() || !disallow.is_empty() => {
                        anyhow::bail!("`--allow` and `--disallow` cannot be used with `--visibility public`")
                    }
                    (Visibility::Private { .. }, Some(EditVisibility::Public)) => {
                        proposal.visibility = Visibility::Public;
                    }
                }
                proposal.delegates = proposal
                    .delegates
                    .into_iter()
                    .chain(delegates)
                    .filter(|d| !rescind.contains(d))
                    .collect::<Vec<_>>();

                verify_project_delegates(&proposal, &current, &repo)?;

                for (id, key, val) in payload {
                    if let Some(ref mut payload) = proposal.payload.get_mut(&id) {
                        if let Some(obj) = payload.as_object_mut() {
                            if val.is_null() {
                                obj.remove(&key);
                            } else {
                                obj.insert(key, val);
                            }
                        } else {
                            anyhow::bail!("payload `{id}` is not a map");
                        }
                    } else {
                        anyhow::bail!("payload `{id}` not found in identity document");
                    }
                }
                proposal
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

            let revision =
                id::propose_changes(&profile, &repo, proposal, &mut identity, title, description)?;
            match revision {
                Some(revision) => {
                    if options.quiet {
                        term::print(revision.id);
                    } else {
                        term::success!(
                            "Identity revision {} created",
                            term::format::tertiary(revision.id)
                        );
                        id::print(&revision, &identity, &repo, &profile)?;
                    }
                }
                None => {
                    if !options.quiet {
                        term::print(term::format::italic(
                            "Nothing to do. The document is up to date. See `rad inspect --identity`.",
                        ));
                    }
                }
            }
        }
        Operation::ListRevisions => {
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
                    term::format::Author::new(r.author.public_key(), &profile).labels();
                let timestamp = term::format::timestamp(r.timestamp).into();

                revisions.push([icon, id, title, alias, author, state, timestamp]);
            }
            revisions.print();
        }
        Operation::RedactRevision { revision } => {
            let revision = get(revision, &identity, &repo)?.clone();
            let signer = term::signer(&profile)?;

            if revision.is_accepted() {
                anyhow::bail!("cannot redact accepted revision");
            }
            if options.interactive.confirm(format!(
                "Redact revision {}?",
                term::format::tertiary(revision.id)
            )) {
                identity.redact(revision.id, &signer)?;

                if !options.quiet {
                    term::success!("Revision {} redacted", revision.id);
                }
            }
        }
        Operation::ShowRevision { revision } => {
            let revision = get(revision, &identity, &repo)?;
            let previous = revision.parent.unwrap_or(revision.id);
            let previous = identity
                .revision(&previous)
                .ok_or(anyhow!("revision `{previous}` not found"))?;

            id::print(revision, previous, &repo, &profile)?;
        }
        Operation::Migrate { title, description } => {
            let proposal = repo.identity_doc()?.doc;
            if options.interactive.confirm(format!(
                "Perform migration?\n{}",
                term::format::tertiary(serde_json::to_string_pretty(&proposal)?)
            )) {
                let revision = id::propose_changes(
                    &profile,
                    &repo,
                    proposal.edit(),
                    &mut identity,
                    title,
                    description,
                )?;
                match revision {
                    Some(revision) => {
                        if options.quiet {
                            term::print(revision.id);
                        } else {
                            term::success!(
                                "Identity revision {} created",
                                term::format::tertiary(revision.id)
                            );
                            id::print(&revision, &identity, &repo, &profile)?;
                        }
                    }
                    None => {
                        if !options.quiet {
                            let msg = "Nothing to do. The document is up to date. See `rad inspect --identity`.";
                            term::print(term::format::italic(msg));
                        }
                    }
                }
            }
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

#[derive(Clone, Debug)]
enum VerificationError {
    MissingDefaultBranch {
        branch: radicle::git::RefString,
        did: Did,
    },
    MissingDelegate {
        did: Did,
    },
}

impl VerificationError {
    fn print(&self) {
        match self {
            VerificationError::MissingDefaultBranch { branch, did } => term::error(format!(
                "missing {} for {} in local storage",
                term::format::secondary(branch),
                term::format::did(did)
            )),
            VerificationError::MissingDelegate { did } => {
                term::error(format!("the delegate {did} is missing"));
                term::hint(format!(
                    "run `rad follow {did}` to follow this missing peer"
                ));
            }
        }
    }
}

// N.b. if we are modifying a project repository, we want to ensure that we have
// rule for the default branch and we can verify the number of delegates for it
fn verify_project_delegates<S>(proposal: &RawDoc, current: &Doc, repo: &S) -> anyhow::Result<()>
where
    S: ReadRepository,
{
    let Some(threshold) = current.default_branch_threshold()? else {
        anyhow::bail!("failed to find canonical ref rule for default branch");
    };

    if let Some(errs) = verify_delegates(&proposal.delegates, threshold, repo)? {
        term::error(format!("failed to verify delegates for {}", repo.rid()));
        term::error(format!(
            "the threshold of {} delegates cannot be met..",
            threshold
        ));
        for e in errs {
            e.print();
        }
        anyhow::bail!("fatal: refusing to update identity document");
    }

    Ok(())
}

fn verify_delegates<'a, I, S>(
    delegates: I,
    threshold: usize,
    repo: &S,
) -> anyhow::Result<Option<Vec<VerificationError>>>
where
    I: IntoIterator<Item = &'a Did>,
    I::IntoIter: ExactSizeIterator,
    S: ReadRepository,
{
    let delegates = delegates.into_iter();
    let n = delegates.len();
    let (canonical, _) = repo.canonical_head()?;
    let mut missing = Vec::with_capacity(n);

    for did in delegates {
        match refs::SignedRefsAt::load((*did).into(), repo)? {
            None => {
                missing.push(VerificationError::MissingDelegate { did: *did });
            }
            Some(refs::SignedRefsAt { sigrefs, .. }) => {
                if sigrefs.get(&canonical).is_none() {
                    missing.push(VerificationError::MissingDefaultBranch {
                        branch: canonical.to_ref_string(),
                        did: *did,
                    });
                }
            }
        }
    }

    Ok((n - missing.len() < threshold).then_some(missing))
}
