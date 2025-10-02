use std::path::Path;

use crate::terminal as term;

/// Pretty-print a JSON value with syntax highlighting.
pub fn to_pretty(value: &impl serde::Serialize, path: &Path) -> anyhow::Result<Vec<term::Line>> {
    let json = serde_json::to_string_pretty(&value)?;
    let mut highlighter = term::highlight::Highlighter::default();
    let highlighted = highlighter.highlight(path, json.as_bytes())?;

    Ok(highlighted)
}
