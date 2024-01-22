use std::str::FromStr;

use radicle::cob::ObjectId;
use radicle::identity::Did;

use serde::de::{Error, Unexpected};
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
pub struct SelectionOutput {
    /// The selected operation.
    operation: Option<String>,
    /// The selected object id(s).
    #[serde(deserialize_with = "deserialize_object_ids")]
    ids: Vec<ObjectId>,
    // Optional CLI args.
    args: Option<Vec<String>>,
}

impl SelectionOutput {
    pub fn with_id(mut self, id: ObjectId) -> Self {
        self.ids.push(id);
        self
    }

    pub fn with_operation(mut self, operation: String) -> Self {
        self.operation = Some(operation);
        self
    }

    pub fn with_args(mut self, args: &[String]) -> Self {
        self.args = Some(args.to_vec());
        self
    }

    pub fn operation(&self) -> Option<&String> {
        self.operation.as_ref()
    }

    pub fn ids(&self) -> &Vec<ObjectId> {
        &self.ids
    }

    pub fn args(&self) -> Option<&Vec<String>> {
        self.args.as_ref()
    }
}

fn deserialize_object_ids<'de, D>(deserializer: D) -> Result<Vec<ObjectId>, D::Error>
where
    D: Deserializer<'de>,
{
    let values: Vec<&str> = Deserialize::deserialize(deserializer)?;
    let mut ids = vec![];

    for val in values {
        let id = ObjectId::from_str(val)
            .map_err(|_| D::Error::invalid_value(Unexpected::Str(val), &"an object id"))?;
        ids.push(id);
    }

    Ok(ids)
}

fn parse_output<'a, T: Deserialize<'a>>(output: &'a str) -> Result<T, TuiError> {
    match serde_json::from_str::<'a, T>(output) {
        Ok(output) => Ok(output),
        Err(err) => Err(TuiError::Parser(err)),
    }
}

pub fn select_patch_id() -> Result<Option<SelectionOutput>, TuiError> {
    let args = ["patch", "select", "--mode", "id"];
    match term::command::rad_tui(args) {
        Ok(Some(output)) => Ok(Some(parse_output(&output)?)),
        Ok(None) => Ok(None),
        Err(err) => Err(TuiError::Command(err)),
    }
}

pub fn select_patch_operation(
    state: &str,
    authored: bool,
    authors: Vec<Did>,
) -> Result<Option<SelectionOutput>, TuiError> {
    let mut args = vec!["patch".to_string(), "select".to_string()];

    args.push(format!("--{}", state));

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
            SelectionOutput::default()
                .with_id(id)
                .with_operation("show".to_string())
                .with_args(&[]),
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
            SelectionOutput::default().with_id(id).with_args(&[]),
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

        assert_eq!(SelectionOutput::default().with_id(id), output);
        Ok(())
    }

    #[test]
    fn parse_selection_output_fails_with_missing_ids() -> Result<(), TuiError> {
        let json = r#"{"operation":null,"args":[]}"#;
        let output: Result<SelectionOutput, TuiError> = parse_output(json);

        assert!(output.is_err());
        Ok(())
    }

    #[test]
    fn parse_selection_output_fails_with_invalid_ids() -> Result<(), TuiError> {
        let json = r#"{"ids":["radicle"]}"#;
        let output: Result<SelectionOutput, TuiError> = parse_output(json);

        assert!(output.is_err());
        Ok(())
    }
}
