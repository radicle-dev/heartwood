use super::Headers;

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("invalid utf-8")]
    Utf8(#[source] std::str::Utf8Error),
    #[error("missing tree")]
    MissingTree,
    #[error("invalid tree")]
    InvalidTree,
    #[error("invalid format")]
    InvalidFormat,
    #[error("invalid parent")]
    InvalidParent,
    #[error("invalid header")]
    InvalidHeader,
    #[error("invalid author")]
    InvalidAuthor,
    #[error("missing author")]
    MissingAuthor,
    #[error("invalid committer")]
    InvalidCommitter,
    #[error("missing committer")]
    MissingCommitter,
}

pub fn parse_commit_header<
    Tree: std::str::FromStr,
    Parent: std::str::FromStr,
    Signature: std::str::FromStr,
>(
    header: &str,
) -> Result<(Tree, Vec<Parent>, Signature, Signature, Headers), ParseError> {
    let mut lines = header.lines();

    let tree = match lines.next() {
        Some(tree) => tree
            .strip_prefix("tree ")
            .map(Tree::from_str)
            .transpose()
            .map_err(|_| ParseError::InvalidTree)?
            .ok_or(ParseError::MissingTree)?,
        None => return Err(ParseError::MissingTree),
    };

    let mut parents = Vec::new();
    let mut author: Option<Signature> = None;
    let mut committer: Option<Signature> = None;
    let mut headers = Headers::new();

    for line in lines {
        // Check if a signature is still being parsed
        if let Some(rest) = line.strip_prefix(' ') {
            let value: &mut String = headers
                .0
                .last_mut()
                .map(|(_, v)| v)
                .ok_or(ParseError::InvalidFormat)?;
            value.push('\n');
            value.push_str(rest);
            continue;
        }

        if let Some((name, value)) = line.split_once(' ') {
            match name {
                "parent" => parents.push(
                    value
                        .parse::<Parent>()
                        .map_err(|_| ParseError::InvalidParent)?,
                ),
                "author" => {
                    author = Some(
                        value
                            .parse::<Signature>()
                            .map_err(|_| ParseError::InvalidAuthor)?,
                    )
                }
                "committer" => {
                    committer = Some(
                        value
                            .parse::<Signature>()
                            .map_err(|_| ParseError::InvalidCommitter)?,
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
