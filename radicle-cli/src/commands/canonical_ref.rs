use std::ffi::OsString;
use std::path::Path;

use anyhow::{anyhow, Context};

use nonempty::NonEmpty;
use radicle::cob;
use radicle::cob::identity::Identity;
use radicle::git::canonical::rules;
use radicle::git::canonical::rules::Rule;
use radicle::git::canonical::RawRule;
use radicle::identity::Did;
use radicle::identity::IdentityMut;
use radicle::identity::RawDoc;
use radicle::identity::RepoId;
use radicle::prelude::Profile;
use radicle::storage::ReadStorage;
use radicle::storage::WriteRepository;
use radicle_term::Element as _;

use crate::id;
use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};
use crate::terminal::json;

pub const HELP: Help = Help {
    name: "cref",
    description: "Manage canonical reference rules",
    version: env!("RADICLE_VERSION"),
    usage: r#"
Usage

    rad cref list [<option>...]
    rad cref add <refspec> [--delegate <did>..] [--threshold <num>] [<option>...]
    rad cref edit <refspec> [--delegate <did>..] [--threshold <num>] [<option>...]
    rad cref remove <refspec> [<option>...]

    The *rad cref* command is used to manage the rules for setting
    canonical references in the Radicle repository.

    The *list* command lists all the rules associated with the identity.

    The *add* command will add the rule for the given *refspec*.

    The *edit* command will amend the rule for the given *refspec*.

    The *remove* command will remove the rule for the given *refspec*.

Add/Edit options

   --delegate <did>        Delegate DID to be used for the canonical reference rule.
                           Can be used multiple times to specify more delegates.
                           If not specified the default '$identity' token will be used.
   --threshold <num>       The threshold number of votes required to make a reference canonical (default: 1)

Options

    --repo <rid>           Repository (defaults to the current repository)
    --quiet, -q            Don't print anything
    --help                 Print help
"#,
};

pub enum Operation {
    Add {
        title: Option<String>,
        description: Option<String>,
        pattern: rules::Pattern,
        delegates: rules::Allowed,
        threshold: usize,
    },
    Edit {
        title: Option<String>,
        description: Option<String>,
        pattern: rules::Pattern,
        delegates: rules::Allowed,
        threshold: usize,
    },
    List,
    Remove {
        title: Option<String>,
        description: Option<String>,
        pattern: rules::Pattern,
    },
}

#[derive(Debug, Default)]
pub enum OperationName {
    Add,
    Edit,
    #[default]
    List,
    Remove,
}

pub struct Options {
    pub op: Operation,
    pub rid: Option<RepoId>,
    pub quiet: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut op: Option<OperationName> = None;
        let mut pattern: Option<rules::Pattern> = None;
        let mut delegates: Vec<Did> = Vec::new();
        let mut threshold: Option<usize> = None;
        let mut rid: Option<RepoId> = None;
        let mut title: Option<String> = None;
        let mut description: Option<String> = None;
        let mut quiet = false;

        while let Some(arg) = parser.next()? {
            match arg {
                Value(val) if op.is_none() => match val.to_string_lossy().as_ref() {
                    "a" | "add" => op = Some(OperationName::Add),
                    "e" | "edit" => op = Some(OperationName::Edit),
                    "r" | "remove" => op = Some(OperationName::Remove),
                    "l" | "list" => op = Some(OperationName::List),

                    unknown => anyhow::bail!("unknown operation '{}'", unknown),
                },

                Value(val)
                    if matches!(
                        op,
                        Some(OperationName::Add | OperationName::Edit | OperationName::Remove)
                    ) =>
                {
                    pattern = Some(rules::Pattern::try_from(
                        term::args::qualified_pattern_string(val)?,
                    )?);
                }

                Long("title")
                    if matches!(
                        op,
                        Some(OperationName::Add | OperationName::Edit | OperationName::Remove)
                    ) =>
                {
                    title = Some(parser.value()?.to_string_lossy().into());
                }
                Long("description")
                    if matches!(
                        op,
                        Some(OperationName::Add | OperationName::Edit | OperationName::Remove)
                    ) =>
                {
                    description = Some(parser.value()?.to_string_lossy().into());
                }
                Long("delegate")
                    if matches!(op, Some(OperationName::Add | OperationName::Edit)) =>
                {
                    let did = term::args::did(&parser.value()?)?;
                    delegates.push(did);
                }
                Long("threshold")
                    if matches!(op, Some(OperationName::Add | OperationName::Edit)) =>
                {
                    threshold = Some(parser.value()?.to_string_lossy().parse()?);
                }
                Long("repo") => {
                    let val = parser.value()?;
                    let val = term::args::rid(&val)?;

                    rid = Some(val);
                }
                Long("quiet") | Short('q') => {
                    quiet = true;
                }
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                _ => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        let op = match op.unwrap_or_default() {
            OperationName::Add => Operation::Add {
                title,
                description,
                pattern: pattern.ok_or_else(|| anyhow!("a refspec must be provided"))?,
                delegates: NonEmpty::from_vec(delegates)
                    .map(rules::Allowed::Set)
                    .unwrap_or(rules::Allowed::Delegates),
                threshold: threshold.unwrap_or(1),
            },
            OperationName::Edit => Operation::Edit {
                title,
                description,
                pattern: pattern.ok_or_else(|| anyhow!("a refspec must be provided"))?,
                delegates: NonEmpty::from_vec(delegates)
                    .map(rules::Allowed::Set)
                    .unwrap_or(rules::Allowed::Delegates),
                threshold: threshold.unwrap_or(1),
            },
            OperationName::Remove => Operation::Remove {
                title,
                description,
                pattern: pattern.ok_or_else(|| anyhow!("a refspec must be provided"))?,
            },
            OperationName::List => Operation::List,
        };

        Ok((Options { op, rid, quiet }, vec![]))
    }
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let profile = ctx.profile()?;
    let storage = &profile.storage;
    let rid = options
        .rid
        .map(Ok)
        .unwrap_or_else(|| radicle::rad::cwd().map(|(_, rid)| rid))?;
    let repo = storage
        .repository(rid)
        .context(anyhow!("repository `{rid}` not found in local storage"))?;
    let mut identity = Identity::load_mut(&repo)?;
    let current = identity.current().clone();

    match options.op {
        Operation::Add {
            title,
            description,
            pattern,
            delegates,
            threshold,
        } => {
            let rule = Rule::new(delegates, threshold);
            let proposal = current.doc.clone().edit();
            add(
                proposal,
                pattern,
                rule,
                &profile,
                &repo,
                &mut identity,
                title,
                description,
                options.quiet,
            )?;
        }
        Operation::Edit {
            title,
            description,
            pattern,
            delegates,
            threshold,
        } => {
            let rule = Rule::new(delegates, threshold);
            let proposal = current.doc.clone().edit();
            edit(
                proposal,
                pattern,
                rule,
                &profile,
                &repo,
                &mut identity,
                title,
                description,
                options.quiet,
            )?;
        }
        Operation::Remove {
            title,
            description,
            pattern,
        } => {
            let proposal = current.doc.clone().edit();
            remove(
                proposal,
                pattern,
                profile,
                &repo,
                &mut identity,
                title,
                description,
                options.quiet,
            )?;
        }
        Operation::List => {
            print_rules(current.rules())?;
        }
    }
    Ok(())
}

fn remove<R>(
    mut proposal: RawDoc,
    pattern: rules::Pattern,
    profile: Profile,
    repo: &R,
    identity: &mut IdentityMut<R>,
    title: Option<String>,
    description: Option<String>,
    quiet: bool,
) -> Result<(), anyhow::Error>
where
    R: cob::Store + WriteRepository,
{
    proposal.canonical_refs.rules.remove(&pattern);
    let revision = id::propose_changes(&profile, repo, proposal, identity, title, description)?;
    match revision {
        Some(revision) => {
            if !quiet {
                term::success!("Rule for {} has been removed", pattern);
                term::success!(
                    "Identity revision {} created",
                    term::format::tertiary(revision.id)
                );
            }
        }
        None => {
            if !quiet {
                term::print(term::format::italic(
                    "Nothing to do. The rules are up to date. See `rad cref list`.",
                ));
            }
        }
    }
    Ok(())
}

fn add<R>(
    mut proposal: RawDoc,
    pattern: rules::Pattern,
    rule: RawRule,
    profile: &Profile,
    repo: &R,
    identity: &mut IdentityMut<R>,
    title: Option<String>,
    description: Option<String>,
    quiet: bool,
) -> Result<(), anyhow::Error>
where
    R: cob::Store + WriteRepository,
{
    proposal.canonical_refs.rules.insert(pattern.clone(), rule);
    let revision = id::propose_changes(profile, repo, proposal, identity, title, description)?;
    match revision {
        Some(revision) => {
            if !quiet {
                term::success!("Rule for {pattern} has been added");
                term::success!(
                    "Identity revision {} created",
                    term::format::tertiary(revision.id)
                );
            }
        }
        None => {
            if !quiet {
                term::print(term::format::italic(
                    "Nothing to do. The rules are up to date. See `rad cref list`.",
                ));
            }
        }
    }
    Ok(())
}

fn edit<R>(
    mut proposal: RawDoc,
    pattern: rules::Pattern,
    rule: RawRule,
    profile: &Profile,
    repo: &R,
    identity: &mut IdentityMut<R>,
    title: Option<String>,
    description: Option<String>,
    quiet: bool,
) -> Result<(), anyhow::Error>
where
    R: cob::Store + WriteRepository,
{
    let changed = proposal.canonical_refs.rules.insert(pattern.clone(), rule);
    if changed.is_none() {
        term::print(term::format::italic(
            "Nothing to do. The rules are up to date. See `rad cref list`.",
        ));
        return Ok(());
    }
    let revision = id::propose_changes(profile, repo, proposal, identity, title, description)?;
    match revision {
        Some(revision) => {
            if !quiet {
                term::success!("Rule for {pattern} has been modified");
                term::success!(
                    "Identity revision {} created",
                    term::format::tertiary(revision.id)
                );
            }
        }
        None => {
            if !quiet {
                term::print(term::format::italic(
                    "Nothing to do. The rules are up to date. See `rad cref list`.",
                ));
            }
        }
    }
    Ok(())
}

fn print_rules(rules: &rules::Rules) -> anyhow::Result<()> {
    json::to_pretty(&rules, Path::new("radicle.json"))?.print();
    Ok(())
}
