use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

use thiserror::Error;

use clap::{Parser, Subcommand};

use radicle::cob;
use radicle::git;
use radicle::prelude::*;
use radicle::storage;

use crate::git::Rev;

#[derive(Parser, Debug)]
#[command(disable_version_flag = true)]
pub struct Args {
    #[command(subcommand)]
    pub(super) command: Command,
}

#[derive(Subcommand, Debug)]
pub(super) enum Command {
    /// Create a new COB of a given type given initial actions
    Create(#[clap(flatten)] Create),

    /// List all COBs of a given type
    List {
        /// Repository ID of the repository to operate on
        #[arg(long, short, value_name = "RID")]
        repo: RepoId,

        /// Typename of the object(s) to list
        #[arg(long = "type", short, value_name = "TYPENAME")]
        type_name: cob::TypeName,
    },

    /// Print a log of all raw operations on a COB
    Log {
        /// Tepository ID of the repository to operate on
        #[arg(long, short, value_name = "RID")]
        repo: RepoId,

        /// Typename of the object(s) to show
        #[arg(long = "type", short, value_name = "TYPENAME")]
        type_name: cob::TypeName,

        /// Object ID of the object to log
        #[arg(long, short, value_name = "OID")]
        object: Rev,

        /// Desired output format
        #[arg(long, default_value_t = Format::Pretty, value_parser = FormatParser)]
        format: Format,

        /// Object ID of the commit of the operation to start iterating at
        #[arg(long, value_name = "OID")]
        from: Option<Rev>,

        /// Object ID of the commit of the operation to stop iterating at
        #[arg(long, value_name = "OID")]
        until: Option<Rev>,
    },

    /// Migrate the COB database to the latest version
    Migrate,

    /// Print the state of COBs
    Show {
        /// Repository ID of the repository to operate on
        #[arg(long, short, value_name = "RID")]
        repo: RepoId,

        /// Typename of the object(s) to show
        #[arg(long = "type", short, value_name = "TYPENAME")]
        type_name: cob::TypeName,

        /// Object ID(s) of the objects to show
        #[arg(long = "object", short, value_name = "OID", action = clap::ArgAction::Append, required = true)]
        objects: Vec<Rev>,

        /// Desired output format
        #[arg(long, default_value_t = Format::Json, value_parser = FormatParser)]
        format: Format,
    },

    /// Add actions to a COB
    Update(#[clap(flatten)] Update),
}

#[derive(Parser, Debug)]
pub(super) struct Operation {
    /// Message describing the operation
    #[arg(long, short)]
    pub(super) message: String,

    /// Supply embed of given name via file at given path
    #[arg(long = "embed-file", value_names = ["NAME", "PATH"], num_args = 2)]
    pub(super) embed_files: Vec<String>,

    /// Supply embed of given name via object ID of blob
    #[arg(long = "embed-hash", value_names = ["NAME", "OID"], num_args = 2)]
    pub(super) embed_hashes: Vec<String>,

    /// A file that contains a sequence actions (in JSONL format) to apply.
    #[arg(value_name = "FILENAME")]
    pub(super) actions: PathBuf,
}

#[derive(Parser, Debug)]
pub(super) struct Create {
    /// Repository ID of the repository to operate on
    #[arg(long, short, value_name = "RID")]
    pub(super) repo: RepoId,

    /// Typename of the object to create
    #[arg(long = "type", short, value_name = "TYPENAME")]
    pub(super) type_name: FilteredTypeName,

    #[clap(flatten)]
    pub(super) operation: Operation,
}

#[derive(Parser, Debug)]
pub(super) struct Update {
    /// Repository ID of the repository to operate on
    #[arg(long, short)]
    pub(super) repo: RepoId,

    /// Typename of the object to update
    #[arg(long = "type", short, value_name = "TYPENAME")]
    pub(super) type_name: FilteredTypeName,

    /// Object ID of the object to update
    #[arg(long, short, value_name = "OID")]
    pub(super) object: Rev,

    // TODO(finto): `Format` is unused and is obsolete for this command
    /// Desired output format
    #[arg(long, default_value_t = Format::Json, value_parser = FormatParser)]
    pub(super) format: Format,

    #[clap(flatten)]
    pub(super) operation: Operation,
}

/// A precursor to [`cob::Embed`] used for parsing
/// that can be initialized without relying on a [`git::Repository`].
#[derive(Clone, Debug)]
pub(super) struct Embed {
    name: String,
    content: EmbedContent,
}

impl Embed {
    pub(super) fn try_into_bytes(
        self,
        repo: &storage::git::Repository,
    ) -> anyhow::Result<cob::Embed<cob::Uri>> {
        Ok(match self.content {
            EmbedContent::Hash(hash) => cob::Embed {
                name: self.name,
                content: hash.resolve::<git::Oid>(&repo.backend)?.into(),
            },
            EmbedContent::Path(path) => {
                cob::Embed::store(self.name, &fs::read(path)?, &repo.backend)?
            }
        })
    }
}

#[derive(Clone, Debug)]
pub(super) enum EmbedContent {
    Path(PathBuf),
    Hash(Rev),
}

impl From<PathBuf> for EmbedContent {
    fn from(path: PathBuf) -> Self {
        EmbedContent::Path(path)
    }
}

impl From<Rev> for EmbedContent {
    fn from(rev: Rev) -> Self {
        EmbedContent::Hash(rev)
    }
}

/// Parses a slice of all embeds as name-path or name-oid pairs as aggregated by
/// `clap`.
/// E.g. `["image", "./image.png", "code", "d87dcfe8c2b3200e78b128d9b959cfdf7063fefe"]`
/// will result a `Vec` of two [`Embed`]s.
///
/// # Panics
///
/// If the length of `values` is not divisible by 2.
pub(super) fn parse_many_embeds<T>(values: &[String]) -> impl Iterator<Item = Embed> + use<'_, T>
where
    T: From<String>,
    EmbedContent: From<T>,
{
    // `clap` ensures we have 2 values per option occurrence,
    // so we can chunk the aggregated slice exactly.
    let chunks = values.chunks_exact(2);

    assert!(chunks.remainder().is_empty());

    chunks.map(|chunk| {
        // Slice accesses will not panic, guaranteed by `chunks_exact(2)`.
        Embed {
            name: chunk[0].to_string(),
            content: EmbedContent::from(T::from(chunk[1].clone())),
        }
    })
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Format {
    Json,
    Pretty,
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Format::Json => f.write_str("json"),
            Format::Pretty => f.write_str("pretty"),
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Error)]
#[error("invalid format value: {0:?}")]
pub struct FormatParseError(String);

impl FromStr for Format {
    type Err = FormatParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "json" => Ok(Self::Json),
            "pretty" => Ok(Self::Pretty),
            _ => Err(FormatParseError(s.to_string())),
        }
    }
}

#[derive(Clone, Debug)]
struct FormatParser;

impl clap::builder::TypedValueParser for FormatParser {
    type Value = Format;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        use clap::error::ErrorKind;

        let format = <Format as std::str::FromStr>::from_str.parse_ref(cmd, arg, value)?;
        match cmd.get_name() {
            "show" | "update" if format == Format::Pretty => Err(clap::Error::raw(
                ErrorKind::ValueValidation,
                format!("output format `{format}` is not allowed in this command"),
            )
            .with_cmd(cmd)),
            _ => Ok(format),
        }
    }

    fn possible_values(
        &self,
    ) -> Option<Box<dyn Iterator<Item = clap::builder::PossibleValue> + '_>> {
        use clap::builder::PossibleValue;
        Some(Box::new(
            [PossibleValue::new("json"), PossibleValue::new("pretty")].into_iter(),
        ))
    }
}

/// A thin wrapper around [`cob::TypeName`] used for parsing.
/// Well known COB type names are captured as variants,
/// with [`FilteredTypeName::Other`] as an escape hatch for type names
/// that are not well known.
#[derive(Clone, Debug)]
pub(super) enum FilteredTypeName {
    Issue,
    Patch,
    Identity,
    Other(cob::TypeName),
}

impl AsRef<cob::TypeName> for FilteredTypeName {
    fn as_ref(&self) -> &cob::TypeName {
        match self {
            FilteredTypeName::Issue => &cob::issue::TYPENAME,
            FilteredTypeName::Patch => &cob::patch::TYPENAME,
            FilteredTypeName::Identity => &cob::identity::TYPENAME,
            FilteredTypeName::Other(value) => value,
        }
    }
}

impl From<cob::TypeName> for FilteredTypeName {
    fn from(value: cob::TypeName) -> Self {
        if value == *cob::issue::TYPENAME {
            FilteredTypeName::Issue
        } else if value == *cob::patch::TYPENAME {
            FilteredTypeName::Patch
        } else if value == *cob::identity::TYPENAME {
            FilteredTypeName::Identity
        } else {
            FilteredTypeName::Other(value)
        }
    }
}

impl std::fmt::Display for FilteredTypeName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl std::str::FromStr for FilteredTypeName {
    type Err = cob::TypeNameParse;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from(s.parse::<cob::TypeName>()?))
    }
}

#[cfg(test)]
mod test {
    use super::Args;
    use clap::error::ErrorKind;
    use clap::Parser;

    const ARGS: &[&str] = &[
        "--repo",
        "rad:z3Tr6bC7ctEg2EHmLvknUr29mEDLH",
        "--type",
        "xyz.radicle.issue",
        "--object",
        "f2de534b5e81d7c6e2dcaf58c3dd91573c0a0354",
    ];

    #[test]
    fn should_allow_log_json_format() {
        let args = Args::try_parse_from(
            ["cob", "log", "--format", "json"]
                .iter()
                .chain(ARGS.iter())
                .collect::<Vec<_>>(),
        );
        assert!(args.is_ok())
    }

    #[test]
    fn should_allow_log_pretty_format() {
        let args = Args::try_parse_from(
            ["cob", "log", "--format", "pretty"]
                .iter()
                .chain(ARGS.iter())
                .collect::<Vec<_>>(),
        );
        assert!(args.is_ok())
    }

    #[test]
    fn should_allow_show_json_format() {
        let args = Args::try_parse_from(
            ["cob", "show", "--format", "json"]
                .iter()
                .chain(ARGS.iter())
                .collect::<Vec<_>>(),
        );
        assert!(args.is_ok())
    }

    #[test]
    fn should_allow_update_json_format() {
        let args = Args::try_parse_from(
            [
                "cob",
                "update",
                "--format",
                "json",
                "--message",
                "",
                "/dev/null",
            ]
            .iter()
            .chain(ARGS.iter())
            .collect::<Vec<_>>(),
        );
        println!("{args:?}");
        assert!(args.is_ok())
    }

    #[test]
    fn should_not_allow_show_pretty_format() {
        let err = Args::try_parse_from(["cob", "show", "--format", "pretty"]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::ValueValidation);
    }

    #[test]
    fn should_not_allow_update_pretty_format() {
        let err = Args::try_parse_from(["cob", "update", "--format", "pretty"]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::ValueValidation);
    }
}
