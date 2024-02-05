use radicle::issue;

use crate::terminal;

use super::*;
use super::{SelectionOutput, TuiError};

#[derive(Default, Debug, PartialEq, Eq)]
pub enum Assigned {
    #[default]
    Me,
    Peer(Did),
}

pub fn select_id() -> Result<Option<SelectionOutput>, TuiError> {
    let args = ["issue", "select", "--mode", "id"];
    match terminal::command::rad_tui(args) {
        Ok(Some(output)) => Ok(Some(parse_output(&output)?)),
        Ok(None) => Ok(None),
        Err(err) => Err(TuiError::Command(err)),
    }
}

pub fn select_operation(
    state: Option<issue::State>,
    assignee: Option<Assigned>,
) -> Result<Option<SelectionOutput>, TuiError> {
    let mut args = vec!["issue".to_string(), "select".to_string()];

    let state = state.map(|s| format!("{s}")).unwrap_or(String::from("all"));
    args.push(format!("--{}", state));

    match assignee {
        Some(Assigned::Me) => {
            args.push("--assigned".to_string());
        }
        Some(Assigned::Peer(did)) => {
            args.push("--assigned".to_string());
            args.push(format!("{did}"));
        }
        _ => {}
    }

    match term::command::rad_tui(args) {
        Ok(Some(output)) => Ok(Some(parse_output(&output)?)),
        Ok(None) => Ok(None),
        Err(err) => Err(TuiError::Command(err)),
    }
}
