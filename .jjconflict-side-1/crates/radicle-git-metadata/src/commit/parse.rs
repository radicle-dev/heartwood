#[cfg(test)]
mod test;

use std::borrow::Cow;

use crate::author::Author;

use super::{
    CommitData,
    headers::Headers,
    trailers::{OwnedTrailer, Token, Trailer},
};

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("the provided commit data contained invalid UTF-8")]
    Utf8(#[source] std::str::Utf8Error),
    #[error("the commit header is missing the 'tree' entry")]
    MissingTree,
    #[error("failed to parse 'tree' value: {0}")]
    InvalidTree(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
    #[error("invalid format: {reason}")]
    InvalidFormat { reason: &'static str },
    #[error("failed to parse 'parent' value: {0}")]
    InvalidParent(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
    #[error("invalid header")]
    InvalidHeader,
    #[error("failed to parse 'author' value: {0}")]
    InvalidAuthor(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
    #[error("the commit header is missing the 'author' entry")]
    MissingAuthor,
    #[error("failed to parse 'committer' value: {0}")]
    InvalidCommitter(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
    #[error("the commit header is missing the 'committer' entry")]
    MissingCommitter,
}

pub(super) fn parse<Tree: std::str::FromStr, Parent: std::str::FromStr>(
    commit: &str,
) -> Result<CommitData<Tree, Parent>, ParseError>
where
    Tree::Err: std::error::Error + Send + Sync + 'static,
    Parent::Err: std::error::Error + Send + Sync + 'static,
{
    // The header and body are separated by the first blank line.
    let (header, body) = commit.split_once("\n\n").ok_or(ParseError::InvalidFormat {
        reason: "commit headers and body must be separated by a blank line",
    })?;

    let (tree, parents, author, committer, headers) =
        parse_headers::<Tree, Parent, Author>(header)?;

    let (message, trailers) = parse_body(body);

    Ok(CommitData {
        tree,
        parents,
        author,
        committer,
        headers,
        message,
        trailers,
    })
}

fn parse_headers<Tree: std::str::FromStr, Parent: std::str::FromStr, Signature: std::str::FromStr>(
    header: &str,
) -> Result<(Tree, Vec<Parent>, Signature, Signature, Headers), ParseError>
where
    Tree::Err: std::error::Error + Send + Sync + 'static,
    Parent::Err: std::error::Error + Send + Sync + 'static,
    Signature::Err: std::error::Error + Send + Sync + 'static,
{
    let mut lines = header.lines();

    let tree = lines
        .next()
        .ok_or(ParseError::MissingTree)?
        .strip_prefix("tree ")
        .map(Tree::from_str)
        .transpose()
        .map_err(|err| ParseError::InvalidTree(Box::new(err)))?
        .ok_or(ParseError::MissingTree)?;

    let mut parents = Vec::new();
    let mut author: Option<Signature> = None;
    let mut committer: Option<Signature> = None;
    let mut headers = Headers::new();

    for line in lines {
        // Check if a signature is still being parsed
        if let Some(rest) = line.strip_prefix(' ') {
            let value: &mut String =
                headers
                    .0
                    .last_mut()
                    .map(|(_, v)| v)
                    .ok_or(ParseError::InvalidFormat {
                        reason: "failed to parse extra header",
                    })?;
            value.push('\n');
            value.push_str(rest);
            continue;
        }

        if let Some((name, value)) = line.split_once(' ') {
            match name {
                "parent" => parents.push(
                    value
                        .parse::<Parent>()
                        .map_err(|err| ParseError::InvalidParent(Box::new(err)))?,
                ),
                "author" => {
                    author = Some(
                        value
                            .parse::<Signature>()
                            .map_err(|err| ParseError::InvalidAuthor(Box::new(err)))?,
                    )
                }
                "committer" => {
                    committer = Some(
                        value
                            .parse::<Signature>()
                            .map_err(|err| ParseError::InvalidCommitter(Box::new(err)))?,
                    )
                }
                _ => headers.push(name, value),
            }
            continue;
        }
    }

    Ok((
        tree,
        parents,
        author.ok_or(ParseError::MissingAuthor)?,
        committer.ok_or(ParseError::MissingCommitter)?,
        headers,
    ))
}

/// Split the commit body (the portion after the first `\n\n` in the object)
/// into a message string and a list of trailers.
///
/// Trailers are only separated out when the last paragraph of the body
/// consists entirely of valid `Token: value` lines. If parsing the last
/// paragraph as trailers fails for any line, the whole body is returned as
/// the message with an empty trailer list.
fn parse_body(body: &str) -> (String, Vec<OwnedTrailer>) {
    // Strip the single trailing newline that Display always writes after the
    // message, so that rfind("\n\n") reliably finds the trailer separator
    // rather than a spurious match at the very end.
    let body = body.trim_end_matches('\n');

    if let Some(split) = body.rfind("\n\n") {
        let candidate = &body[split + 2..];
        // Only treat non-empty paragraphs as trailers.
        if !candidate.trim().is_empty() {
            if let Some(trailers) = try_parse_trailers(candidate) {
                return (body[..split].to_string(), trailers);
            }
        }
    }

    (body.to_string(), Vec::new())
}

/// Attempt to parse every non-empty line in `s` as a `Token: value` trailer.
///
/// Returns `None` if any line is not a valid trailer, so that the caller can
/// fall back to treating the whole paragraph as part of the message.
fn try_parse_trailers(s: &str) -> Option<Vec<OwnedTrailer>> {
    s.lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let (token_str, value) = line.split_once(": ")?;
            let token = Token::try_from(token_str).ok()?;
            // Round-trip through Trailer so that OwnedToken construction
            // stays inside the trailers module where the private field lives.
            Some(
                Trailer {
                    token,
                    value: Cow::Borrowed(value),
                }
                .to_owned(),
            )
        })
        .collect()
}
