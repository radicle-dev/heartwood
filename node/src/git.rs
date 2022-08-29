use std::str::FromStr;

use crate::collections::HashMap;
use crate::identity::UserId;
use crate::storage::{Remote, Remotes, Unverified};
use git_ref_format as format;
use git_url::Url;

/// Default port of the `git` transport protocol.
pub const PROTOCOL_PORT: u16 = 9418;

#[derive(thiserror::Error, Debug)]
pub enum RefError {
    #[error("invalid ref name '{0}'")]
    InvalidName(format::RefString),
    #[error("invalid ref format: {0}")]
    Format(#[from] format::Error),
}

#[derive(thiserror::Error, Debug)]
pub enum ListRefsError {
    #[error("git error: {0}")]
    Git(#[from] git2::Error),
    #[error("invalid ref: {0}")]
    InvalidRef(#[from] RefError),
}

/// List remote refs of a project, given the remote URL.
pub fn list_remotes(url: &Url) -> Result<Remotes<Unverified>, ListRefsError> {
    let url = url.to_string();
    let mut remotes = HashMap::default();
    let mut remote = git2::Remote::create_detached(&url)?;

    remote.connect(git2::Direction::Fetch)?;

    let refs = remote.list()?;
    for r in refs {
        let (id, refname) = parse_ref::<UserId>(r.name())?;
        let entry = remotes
            .entry(id.clone())
            .or_insert_with(|| Remote::new(id, HashMap::default()));

        entry.refs.insert(refname.to_string(), r.oid().into());
    }

    Ok(Remotes::new(remotes))
}

/// Parse a ref string.
pub fn parse_ref<T: FromStr>(s: &str) -> Result<(T, format::RefString), RefError> {
    let input = format::RefStr::try_from_str(s)?;
    let suffix = input
        .strip_prefix(format::refname!("refs/namespaces"))
        .ok_or_else(|| RefError::InvalidName(input.to_owned()))?;

    let mut components = suffix.components();
    let id = components
        .next()
        .ok_or_else(|| RefError::InvalidName(input.to_owned()))?;
    let id = T::from_str(&id.to_string()).map_err(|_| RefError::InvalidName(input.to_owned()))?;
    let refstr = components.collect::<format::RefString>();

    Ok((id, refstr))
}
