use super::*;
use super::{SelectionOutput, TuiError};

pub fn select_id() -> Result<Option<SelectionOutput>, TuiError> {
    let args = ["patch", "select", "--mode", "id"];
    match term::command::rad_tui(args) {
        Ok(Some(output)) => Ok(Some(parse_output(&output)?)),
        Ok(None) => Ok(None),
        Err(err) => Err(TuiError::Command(err)),
    }
}

pub fn select_operation(
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
