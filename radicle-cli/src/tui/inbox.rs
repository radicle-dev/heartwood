use crate::commands::rad_inbox::SortBy;

use super::*;
use super::{SelectionOutput, TuiError};

pub fn select_operation(
    sort_by: SortBy,
) -> Result<Option<SelectionOutput<NotificationId>>, TuiError> {
    let mut args = vec!["inbox".to_string(), "select".to_string()];

    args.push("--sort-by".to_string());
    args.push(sort_by.field.to_string());

    if sort_by.reverse {
        args.push("--reverse".to_string());
    }

    match term::command::rad_tui(args) {
        Ok(Some(output)) => Ok(Some(parse_output(&output)?)),
        Ok(None) => Ok(None),
        Err(err) => Err(TuiError::Command(err)),
    }
}
