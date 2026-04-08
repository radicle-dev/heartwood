use crate::commit::parse::{ParseError, parse};

/// Helper type whose FromStr always fails.
///
/// Used to exercise ParseError::InvalidTree and ParseError::InvalidParent
/// without relying on a specific OID type; String::from_str is infallible so
/// it cannot trigger those variants on its own.
#[derive(Debug)]
struct AlwaysFails;

#[derive(Debug)]
struct AlwaysFailsErr;

impl std::fmt::Display for AlwaysFailsErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "always fails to parse")
    }
}

impl std::error::Error for AlwaysFailsErr {}

impl std::str::FromStr for AlwaysFails {
    type Err = AlwaysFailsErr;

    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        Err(AlwaysFailsErr)
    }
}

#[test]
fn missing_header_body_separator() {
    // No blank line separating the headers from the body at all.
    let raw = "tree abc123\nauthor Alice Liddell <alice@example.com> 1700000000 +0000\ncommitter Alice Liddell <alice@example.com> 1700000000 +0000\nno blank line follows";

    let err = parse::<String, String>(raw).unwrap_err();
    assert!(
        matches!(err, ParseError::InvalidFormat { .. }),
        "unexpected error: {err}"
    );
}

#[test]
fn missing_tree_empty_header() {
    // Header section is empty (just "\n\n").
    let raw = "\n\nMessage";

    let err = parse::<String, String>(raw).unwrap_err();
    assert!(
        matches!(err, ParseError::MissingTree),
        "unexpected error: {err}"
    );
}

#[test]
fn missing_tree_wrong_first_line() {
    // Header section exists but does not open with "tree <oid>".
    let raw = "author Alice Liddell <alice@example.com> 1700000000 +0000\ncommitter Alice Liddell <alice@example.com> 1700000000 +0000\n\nMessage";

    let err = parse::<String, String>(raw).unwrap_err();
    assert!(
        matches!(err, ParseError::MissingTree),
        "unexpected error: {err}"
    );
}

#[test]
fn invalid_tree() {
    // Tree value present but cannot be parsed into the target Tree type.
    let raw = "tree abc123\nauthor Alice Liddell <alice@example.com> 1700000000 +0000\ncommitter Alice Liddell <alice@example.com> 1700000000 +0000\n\nMessage";

    let err = parse::<AlwaysFails, String>(raw).unwrap_err();
    assert!(
        matches!(err, ParseError::InvalidTree(_)),
        "unexpected error: {err}"
    );
}

#[test]
fn invalid_parent() {
    // Parent value present but cannot be parsed into the target Parent type.
    let raw = "tree abc123\nparent bad-oid\nauthor Alice Liddell <alice@example.com> 1700000000 +0000\ncommitter Alice Liddell <alice@example.com> 1700000000 +0000\n\nMessage";

    let err = parse::<String, AlwaysFails>(raw).unwrap_err();
    assert!(
        matches!(err, ParseError::InvalidParent(_)),
        "unexpected error: {err}"
    );
}

#[test]
fn invalid_format_continuation_without_preceding_header() {
    // A continuation line (leading space) appearing before any extra header
    // has been pushed to the headers vec triggers InvalidFormat, because
    // there is no preceding header value to fold it into.
    //
    // Note: author and committer do not push to the headers vec, so a
    // continuation line immediately after them (with an otherwise empty
    // headers vec) exercises this branch.
    let raw = "tree abc123\nauthor Alice Liddell <alice@example.com> 1700000000 +0000\ncommitter Alice Liddell <alice@example.com> 1700000000 +0000\n spurious continuation\n\nMessage";

    let err = parse::<String, String>(raw).unwrap_err();
    assert!(
        matches!(err, ParseError::InvalidFormat { .. }),
        "unexpected error: {err}"
    );
}

#[test]
fn missing_author() {
    let raw =
        "tree abc123\ncommitter Alice Liddell <alice@example.com> 1700000000 +0000\n\nMessage";

    let err = parse::<String, String>(raw).unwrap_err();
    assert!(
        matches!(err, ParseError::MissingAuthor),
        "unexpected error: {err}"
    );
}

#[test]
fn invalid_author() {
    // Author line is present but its value is not a valid Author string
    // (no email bracket, no timestamp, no timezone offset).
    let raw = "tree abc123\nauthor not-a-valid-author\ncommitter Alice Liddell <alice@example.com> 1700000000 +0000\n\nMessage";

    let err = parse::<String, String>(raw).unwrap_err();
    assert!(
        matches!(err, ParseError::InvalidAuthor(_)),
        "unexpected error: {err}"
    );
}

#[test]
fn missing_committer() {
    let raw = "tree abc123\nauthor Alice Liddell <alice@example.com> 1700000000 +0000\n\nMessage";

    let err = parse::<String, String>(raw).unwrap_err();
    assert!(
        matches!(err, ParseError::MissingCommitter),
        "unexpected error: {err}"
    );
}

#[test]
fn invalid_committer() {
    // Committer line is present but its value is not a valid Author string.
    let raw = "tree abc123\nauthor Alice Liddell <alice@example.com> 1700000000 +0000\ncommitter not-a-valid-committer\n\nMessage";

    let err = parse::<String, String>(raw).unwrap_err();
    assert!(
        matches!(err, ParseError::InvalidCommitter(_)),
        "unexpected error: {err}"
    );
}
