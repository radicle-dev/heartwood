use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

use radicle::cob::ObjectId;
use radicle::identity::Did;

use serde::de;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::terminal as term;
use term::command::CommandError;

#[derive(thiserror::Error, Debug)]
pub enum TuiError {
    #[error("error running TUI command: {0}")]
    Command(#[from] CommandError),
    #[error("error parsing TUI output: {0}")]
    Parser(#[from] serde_json::Error),
}

/// The output that should be returned by selection interfaces.
/// Structs of this type are being parsed and instanced from JSON.
#[derive(Clone, Default, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection<Id>
where
    Id: FromStr,
    Id::Err: fmt::Display,
{
    /// The selected operation.
    operation: Option<String>,
    /// The selected id(s).
    #[serde(deserialize_with = "deserialize_ids")]
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
}

fn deserialize_ids<'de, D, Id>(deserializer: D) -> Result<Vec<Id>, D::Error>
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

fn parse_output<'a, T: Deserialize<'a>>(output: &'a str) -> Result<T, TuiError> {
    match serde_json::from_str::<'a, T>(output) {
        Ok(output) => Ok(output),
        Err(err) => Err(TuiError::Parser(err)),
    }
}

pub fn select_patch_id() -> Result<Option<Selection<ObjectId>>, TuiError> {
    let args = ["patch", "select", "--mode", "id"];
    match term::command::rad_tui(args) {
        Ok(Some(output)) => Ok(Some(parse_output(&output)?)),
        Ok(None) => Ok(None),
        Err(err) => Err(TuiError::Command(err)),
    }
}

pub fn select_patch_operation(
    status: &str,
    authored: bool,
    authors: Vec<Did>,
) -> Result<Option<Selection<ObjectId>>, TuiError> {
    let mut args = vec!["patch".to_string(), "select".to_string()];

    args.push(format!("--{}", status));

    if authored {
        args.push("--authored".to_string());
    }
    for author in authors {
        args.push("--author".to_string());
        args.push(format!("{author}"));
    }

    match term::command::rad_tui(args) {
        Ok(Some(output)) => Ok(Some(parse_output(&output)?)),
        Ok(None) => Ok(None),
        Err(err) => Err(TuiError::Command(err)),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse_selection_output_succeeds() -> Result<(), TuiError> {
        let id = ObjectId::from_str("e65863e71192c107282fbc3170b1ad11b6593b37")
            .expect("Cannot parse object id");

        let json =
            r#"{"operation":"show","ids":["e65863e71192c107282fbc3170b1ad11b6593b37"],"args":[]}"#;
        let output = parse_output(json)?;

        assert_eq!(
            Selection {
                ids: vec![id],
                operation: Some("show".to_string()),
                args: Some(vec![]),
            },
            output
        );

        Ok(())
    }

    #[test]
    fn parse_selection_output_succeeds_with_missing_operation() -> Result<(), TuiError> {
        let id = ObjectId::from_str("e65863e71192c107282fbc3170b1ad11b6593b37")
            .expect("Cannot parse object id");

        let json = r#"{"ids":["e65863e71192c107282fbc3170b1ad11b6593b37"],"args":[]}"#;
        let output = parse_output(json)?;

        assert_eq!(
            Selection {
                ids: vec![id],
                operation: None,
                args: Some(vec![]),
            },
            output
        );
        Ok(())
    }

    #[test]
    fn parse_selection_output_succeeds_with_missing_args() -> Result<(), TuiError> {
        let id = ObjectId::from_str("e65863e71192c107282fbc3170b1ad11b6593b37")
            .expect("Cannot parse object id");

        let json = r#"{"ids":["e65863e71192c107282fbc3170b1ad11b6593b37"]}"#;
        let output = parse_output(json)?;

        assert_eq!(
            Selection {
                ids: vec![id],
                operation: None,
                args: None,
            },
            output
        );
        Ok(())
    }

    #[test]
    fn parse_selection_output_fails_with_missing_ids() -> Result<(), TuiError> {
        let json = r#"{"operation":null,"args":[]}"#;
        let output: Result<Selection<ObjectId>, TuiError> = parse_output(json);

        assert!(output.is_err());
        Ok(())
    }

    #[test]
    fn parse_selection_output_fails_with_invalid_ids() -> Result<(), TuiError> {
        let json = r#"{"ids":["radicle"]}"#;
        let output: Result<Selection<ObjectId>, TuiError> = parse_output(json);

        assert!(output.is_err());
        Ok(())
    }
}
