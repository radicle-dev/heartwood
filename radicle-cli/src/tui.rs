use std::ffi::OsString;
use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

use radicle::identity::Did;

use serde::de;
use serde::{Deserialize, Deserializer};

use crate::commands::rad_issue::Assigned;
use crate::terminal as term;
use term::command::CommandError;

// use self::issue::Assigned;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("error running TUI command: {0}")]
    Command(#[from] CommandError),
    #[error("error parsing TUI output: {0}")]
    Parser(#[from] serde_json::Error),
}

/// A `Selection` is expected to be constructed from output of calls
/// to TUI subprpcesses via JSON deserialization. For example, running
/// `rad patch` spawns a subprocess with `rad-tui patch select` and expects
/// a JSON output that can be deserialized into a `Selection`.
/// Note that the `Id` parameter must implement `FromStr` so that it can
/// be parsed during this deserialization.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection<Id>
where
    Id: FromStr,
    Id::Err: fmt::Display,
{
    /// The selected operation.
    operation: Option<String>,
    /// The selected id(s).
    #[serde(deserialize_with = "Selection::deserialize_ids")]
    ids: Vec<Id>,
    // Optional CLI args.
    args: Option<Vec<String>>,
}

impl<Id> Selection<Id>
where
    Id::Err: fmt::Display,
    Id: FromStr,
{
    pub fn operation(&self) -> Option<&String> {
        self.operation.as_ref()
    }

    pub fn ids(&self) -> &Vec<Id> {
        &self.ids
    }

    pub fn args(&self) -> Option<&Vec<String>> {
        self.args.as_ref()
    }

    pub fn from_command(command: Command) -> Result<Option<Self>, Error> {
        match command.run() {
            Ok(Some(output)) => Ok(parse(&output)?),
            Ok(None) => Ok(None),
            Err(err) => Err(err),
        }
    }

    fn deserialize_ids<'de, D>(deserializer: D) -> Result<Vec<Id>, D::Error>
    where
        D: Deserializer<'de>,
        Id: FromStr,
        Id::Err: fmt::Display,
    {
        struct IdsVisitor<Id>(PhantomData<Id>);

        impl<'de, Id> de::Visitor<'de> for IdsVisitor<Id>
        where
            Id: FromStr,
            Id::Err: fmt::Display,
        {
            type Value = Vec<Id>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a list of selectable identifiers")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                use serde::de::Error;

                let mut ids = seq
                    .size_hint()
                    .map_or_else(|| Vec::new(), |n| Vec::with_capacity(n));
                while let Some(id) = seq.next_element::<String>()? {
                    ids.push(id.parse().map_err(A::Error::custom)?);
                }
                Ok(ids)
            }
        }

        deserializer.deserialize_seq(IdsVisitor(PhantomData))
    }
}

impl<Id> Default for Selection<Id>
where
    Id::Err: fmt::Display,
    Id: FromStr,
{
    fn default() -> Self {
        Self {
            operation: None,
            ids: vec![],
            args: None,
        }
    }
}

/// A `Command` defines a set of arguments and executes `rad-tui` with these
/// when `run()` is called.
pub enum Command {
    /// Run patch id selection.
    PatchSelectId,
    /// Run patch operation and id selection with the given filter applied.
    PatchSelectOperation {
        status: String,
        authored: bool,
        authors: Vec<Did>,
    },
    /// Run issue id selection.
    IssueSelectId,
    /// Run issue operation and id selection with the given filter applied.
    IssueSelectOperation {
        state: String,
        assigned: Option<Assigned>,
    },
}

impl Command {
    /// Returns the required and potentially mapped arguments for a call to `rad-tui`
    fn args(&self) -> Vec<OsString> {
        match self {
            Command::PatchSelectId => [
                "patch".into(),
                "select".into(),
                "--mode".into(),
                "id".into(),
            ]
            .to_vec(),
            Command::PatchSelectOperation {
                status,
                authored,
                authors,
            } => {
                let mut args: Vec<OsString> = vec!["patch".into(), "select".into()];

                args.push(format!("--{}", status).into());

                if *authored {
                    args.push("--authored".into());
                }
                for author in authors {
                    args.push("--author".into());
                    args.push(format!("{author}").into());
                }

                args
            }
            Command::IssueSelectId => [
                "issue".into(),
                "select".into(),
                "--mode".into(),
                "id".into(),
            ]
            .to_vec(),
            Command::IssueSelectOperation { state, assigned } => {
                let mut args: Vec<OsString> = vec!["issue".into(), "select".into()];

                args.push(format!("--{}", state).into());

                match assigned {
                    Some(Assigned::Me) => {
                        args.push("--assigned".into());
                    }
                    Some(Assigned::Peer(did)) => {
                        args.push("--assigned".into());
                        args.push(format!("{did}").into());
                    }
                    _ => {}
                }

                args
            }
        }
    }

    /// Runs `rad-tui` with this `Command`'s arguments.
    fn run(&self) -> Result<Option<String>, Error> {
        term::command::rad_tui(self.args()).map_err(Error::Command)
    }
}

/// Deserializes the output of `rad-tui` and constructs the desired type.
fn parse<'a, T: Deserialize<'a>>(output: &'a str) -> Result<T, Error> {
    match serde_json::from_str::<'a, T>(output) {
        Ok(output) => Ok(output),
        Err(err) => Err(Error::Parser(err)),
    }
}

pub fn is_installed() -> bool {
    use std::io::ErrorKind;
    use std::process::{Command, Stdio};

    let not_found = Command::new(term::command::RAD_TUI)
        .stdout(Stdio::null())
        .spawn()
        .is_err_and(|err| err.kind() == ErrorKind::NotFound);

    !not_found
}

pub fn installation_hint() {
    term::hint("An experimental TUI can be enabled by installing `rad-tui`. You can download it from https://files.radicle.xyz/.");
}

#[cfg(test)]
mod test {
    use super::*;
    use radicle::cob::ObjectId;

    #[test]
    fn parse_selection_output_succeeds() -> Result<(), Error> {
        let id = ObjectId::from_str("e65863e71192c107282fbc3170b1ad11b6593b37")
            .expect("Cannot parse object id");

        let json =
            r#"{"operation":"show","ids":["e65863e71192c107282fbc3170b1ad11b6593b37"],"args":[]}"#;
        let selection = parse(json)?;

        assert_eq!(
            Selection {
                ids: vec![id],
                operation: Some("show".to_string()),
                args: Some(vec![]),
            },
            selection
        );

        Ok(())
    }

    #[test]
    fn parse_selection_output_succeeds_with_missing_operation() -> Result<(), Error> {
        let id = ObjectId::from_str("e65863e71192c107282fbc3170b1ad11b6593b37")
            .expect("Cannot parse object id");

        let json = r#"{"ids":["e65863e71192c107282fbc3170b1ad11b6593b37"],"args":[]}"#;
        let selection = parse(json)?;

        assert_eq!(
            Selection {
                ids: vec![id],
                operation: None,
                args: Some(vec![]),
            },
            selection
        );
        Ok(())
    }

    #[test]
    fn parse_selection_output_succeeds_with_missing_args() -> Result<(), Error> {
        let id = ObjectId::from_str("e65863e71192c107282fbc3170b1ad11b6593b37")
            .expect("Cannot parse object id");

        let json = r#"{"ids":["e65863e71192c107282fbc3170b1ad11b6593b37"]}"#;
        let selection = parse(json)?;

        assert_eq!(
            Selection {
                ids: vec![id],
                operation: None,
                args: None,
            },
            selection
        );
        Ok(())
    }

    #[test]
    fn parse_selection_output_fails_with_missing_ids() -> Result<(), Error> {
        let json = r#"{"operation":null,"args":[]}"#;
        let selection: Result<Selection<ObjectId>, Error> = parse(json);

        assert!(selection.is_err());
        Ok(())
    }

    #[test]
    fn parse_selection_output_fails_with_invalid_ids() -> Result<(), Error> {
        let json = r#"{"ids":["radicle"]}"#;
        let selection: Result<Selection<ObjectId>, Error> = parse(json);

        assert!(selection.is_err());
        Ok(())
    }
}
