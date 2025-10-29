use std::io;
use std::str::FromStr;

use clap::{Parser, Subcommand};

use serde_json as json;

use thiserror::Error;

use radicle::cob::{Title, TypeNameParse};
use radicle::identity::doc::update::EditVisibility;
use radicle::identity::doc::update::PayloadUpsert;
use radicle::identity::doc::PayloadId;
use radicle::prelude::{Did, RepoId};

use crate::git::Rev;

use crate::terminal::Interactive;

const ABOUT: &str = "Manage repository identities";
const LONG_ABOUT: &str = r#"
The `id` command is used to manage and propose changes to the
identity of a Radicle repository.

See the rad-id(1) man page for more information.
"#;

#[derive(Debug, Error)]
pub enum PayloadUpsertParseError {
    #[error("could not parse payload id: {0}")]
    IdParse(#[from] TypeNameParse),
    #[error("could not parse json value: {0}")]
    Value(#[from] json::Error),
}

/// Parses a slice of all payload upserts as aggregated by `clap`
/// (see [`Command::Update::payload`]).
/// E.g. `["com.example.one", "name", "1", "com.example.two", "name2", "2"]`
/// will result in iterator over two [`PayloadUpsert`]s.
///
/// # Panics
///
/// If the length of `values` is not divisible by 3.
/// (To catch errors in the definition of the parser derived from
/// [`Command::Update`] or `clap` itself, and unexpected changes to
/// `clap`s behaviour in the future.)
pub(super) fn parse_many_upserts(
    values: &[String],
) -> impl Iterator<Item = Result<PayloadUpsert, PayloadUpsertParseError>> + use<'_> {
    // `clap` ensures we have 3 values per option occurrence,
    // so we can chunk the aggregated slice exactly.
    let chunks = values.chunks_exact(3);

    assert!(chunks.remainder().is_empty());

    chunks.map(|chunk| {
        // Slice accesses will not panic, guaranteed by `chunks_exact(3)`.
        Ok(PayloadUpsert {
            id: PayloadId::from_str(&chunk[0])?,
            key: chunk[1].to_owned(),
            value: json::from_str(&chunk[2].to_owned())?,
        })
    })
}

#[derive(Clone, Debug)]
struct EditVisibilityParser;

impl clap::builder::TypedValueParser for EditVisibilityParser {
    type Value = EditVisibility;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        <EditVisibility as std::str::FromStr>::from_str.parse_ref(cmd, arg, value)
    }

    fn possible_values(
        &self,
    ) -> Option<Box<dyn Iterator<Item = clap::builder::PossibleValue> + '_>> {
        use clap::builder::PossibleValue;
        Some(Box::new(
            [PossibleValue::new("private"), PossibleValue::new("public")].into_iter(),
        ))
    }
}

#[derive(Debug, Parser)]
#[command(about = ABOUT, long_about = LONG_ABOUT, disable_version_flag = true)]
pub struct Args {
    #[command(subcommand)]
    pub(super) command: Option<Command>,

    /// Specify the repository to operate on. Defaults to the current repository
    ///
    /// [example values: rad:z3Tr6bC7ctEg2EHmLvknUr29mEDLH, z3Tr6bC7ctEg2EHmLvknUr29mEDLH]
    #[arg(long)]
    #[arg(value_name = "RID", global = true)]
    pub(super) repo: Option<RepoId>,

    /// Do not ask for confirmation
    #[arg(long)]
    #[arg(global = true)]
    no_confirm: bool,

    /// Suppress output
    #[arg(long, short)]
    #[arg(global = true)]
    pub(super) quiet: bool,
}

impl Args {
    pub(super) fn interactive(&self) -> Interactive {
        if self.no_confirm {
            Interactive::No
        } else {
            Interactive::new(io::stdout())
        }
    }
}

#[derive(Subcommand, Debug)]
pub(super) enum Command {
    /// Accept a proposed revision to the identity document
    #[clap(alias("a"))]
    Accept {
        /// Proposed revision to accept
        #[arg(value_name = "REVISION_ID")]
        revision: Rev,
    },

    /// Reject a proposed revision to the identity document
    #[clap(alias("r"))]
    Reject {
        /// Proposed revision to reject
        #[arg(value_name = "REVISION_ID")]
        revision: Rev,
    },

    /// Edit an existing revision to the identity document
    #[clap(alias("e"))]
    Edit {
        /// Proposed revision to edit
        #[arg(value_name = "REVISION_ID")]
        revision: Rev,

        /// Title of the edit
        #[arg(long)]
        title: Option<Title>,

        /// Description of the edit
        #[arg(long)]
        description: Option<String>,
    },

    /// Propose a new revision to the identity document
    #[clap(alias("u"))]
    Update {
        /// Set the title for the new proposal
        #[arg(long)]
        title: Option<Title>,

        /// Set the description for the new proposal
        #[arg(long)]
        description: Option<String>,

        /// Update the identity by adding a new delegate, identified by their DID
        #[arg(long, short)]
        #[arg(value_name = "DID")]
        #[arg(action = clap::ArgAction::Append)]
        delegate: Vec<Did>,

        /// Update the identity by removing a delegate, identified by their DID
        #[arg(long, short)]
        #[arg(value_name = "DID")]
        #[arg(action = clap::ArgAction::Append)]
        rescind: Vec<Did>,

        /// Update the identity by setting the number of delegates required to accept a revision
        #[arg(long)]
        threshold: Option<usize>,

        /// Update the identity by setting the repository's visibility to private or public
        #[arg(long)]
        #[arg(value_parser = EditVisibilityParser)]
        visibility: Option<EditVisibility>,

        /// Update the identity by giving a specific DID access to a private repository
        #[arg(long)]
        #[arg(value_name = "DID")]
        #[arg(action = clap::ArgAction::Append)]
        allow: Vec<Did>,

        /// Update the identity by removing a specific DID's access from a private repository
        #[arg(long)]
        #[arg(value_name = "DID")]
        #[arg(action = clap::ArgAction::Append)]
        disallow: Vec<Did>,

        /// Update the identity by setting metadata in one of the identity payloads
        ///
        /// [example values: xyz.radicle.project name '"radicle-example"']
        // TODO(erikili:) Value parsers do not operate on series of values, yet. This will
        // change with clap v5, so we can hopefully use `Vec<Payload>`.
        // - https://github.com/clap-rs/clap/discussions/5930#discussioncomment-12315889
        // - https://docs.rs/clap/latest/clap/_derive/index.html#arg-types
        #[arg(long)]
        #[arg(value_names = ["TYPE", "KEY", "VALUE"], num_args = 3)]
        payload: Vec<String>,

        /// Opens your $EDITOR to edit the JSON contents directly
        #[arg(long)]
        edit: bool,
    },

    /// Lists all proposed revisions to the identity document
    #[clap(alias("l"))]
    List,

    /// Show a specific identity proposal
    #[clap(alias("s"))]
    Show {
        /// Proposed revision to show
        #[arg(value_name = "REVISION_ID")]
        revision: Rev,
    },

    /// Redact a revision
    #[clap(alias("d"))]
    Redact {
        /// Proposed revision to redact
        #[arg(value_name = "REVISION_ID")]
        revision: Rev,
    },
}

#[cfg(test)]
mod test {
    use super::{parse_many_upserts, Args};
    use clap::error::ErrorKind;
    use clap::Parser;

    #[test]
    fn should_parse_single_payload() {
        let args = Args::try_parse_from(["id", "update", "--payload", "key", "name", "value"]);
        assert!(args.is_ok())
    }

    #[test]
    fn should_not_parse_single_payload() {
        let err = Args::try_parse_from(["id", "update", "--payload", "key", "name"]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::WrongNumberOfValues);
    }

    #[test]
    fn should_parse_multiple_payloads() {
        let args = Args::try_parse_from([
            "id",
            "update",
            "--payload",
            "key_1",
            "name_1",
            "value_1",
            "--payload",
            "key_2",
            "name_2",
            "value_2",
        ]);
        assert!(args.is_ok())
    }

    #[test]
    fn should_not_parse_single_payloads() {
        let err = Args::try_parse_from([
            "id",
            "update",
            "--payload",
            "key_1",
            "name_1",
            "value_1",
            "--payload",
            "key_2",
            "name_2",
        ])
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::WrongNumberOfValues);
    }

    #[test]
    fn should_not_clobber_payload_args() {
        let err = Args::try_parse_from([
            "id",
            "update",
            "--payload",
            "key_1",
            "name_1",
            "--payload", // ensure `--payload is not treated as an argument`
            "key_2",
            "name_2",
            "value_2",
        ])
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::WrongNumberOfValues);
    }

    #[test]
    fn should_parse_into_payload() {
        let payload: Result<Vec<_>, _> = parse_many_upserts(&[
            "xyz.radicle.project".to_string(),
            "name".to_string(),
            "{}".to_string(),
        ])
        .collect();
        assert!(payload.is_ok())
    }

    #[test]
    #[should_panic(expected = "assertion failed: chunks.remainder().is_empty()")]
    fn should_not_parse_into_payload() {
        let _: Result<Vec<_>, _> =
            parse_many_upserts(&["xyz.radicle.project".to_string(), "name".to_string()]).collect();
    }
}
