use crate::commit::parse::parse;

#[test]
fn root_commit() {
    let raw = r#"tree abc123
author Alice Liddell <alice@example.com> 1700000000 +0000
committer Alice Liddell <alice@example.com> 1700000000 +0000

Initial commit"#;

    let commit = parse::<String, String>(raw).unwrap();

    assert_eq!(commit.tree(), "abc123");
    assert_eq!(commit.parents().count(), 0);
    assert_eq!(commit.author().name, "Alice Liddell");
    assert_eq!(commit.author().email, "alice@example.com");
    assert_eq!(commit.author().time.seconds(), 1700000000);
    assert_eq!(commit.author().time.offset(), 0);
    assert_eq!(commit.committer().name, "Alice Liddell");
    assert_eq!(commit.message(), "Initial commit");
    assert_eq!(commit.trailers().count(), 0);
    assert_eq!(commit.headers().count(), 0);
}

#[test]
fn commit_with_single_parent() {
    let raw = r#"tree def456
parent abc123
author Alice Liddell <alice@example.com> 1700000000 +0000
committer Alice Liddell <alice@example.com> 1700000000 +0000

Second commit"#;

    let commit = parse::<String, String>(raw).unwrap();

    assert_eq!(commit.tree(), "def456");
    assert_eq!(commit.parents().collect::<Vec<_>>(), ["abc123"]);
    assert_eq!(commit.message(), "Second commit");
}

#[test]
fn merge_commit() {
    let raw = r#"tree ghi789
parent abc123
parent def456
author Alice Liddell <alice@example.com> 1700000000 +0000
committer Alice Liddell <alice@example.com> 1700000000 +0000

Merge branch 'feature'"#;

    let commit = parse::<String, String>(raw).unwrap();

    assert_eq!(commit.parents().collect::<Vec<_>>(), ["abc123", "def456"]);
}

#[test]
fn commit_with_multiline_gpgsig() {
    // gpgsig continuation lines are indented by one space in the raw object.
    // The parser folds them back into the header value with embedded newlines.
    let raw = r#"tree abc123
author Alice Liddell <alice@example.com> 1700000000 +0000
committer Alice Liddell <alice@example.com> 1700000000 +0000
gpgsig -----BEGIN SSH SIGNATURE-----
 AAAAB3NzaC1yc2EAAAADAQAB
 AAAA==
 -----END SSH SIGNATURE-----

Signed commit"#;

    let commit = parse::<String, String>(raw).unwrap();

    assert_eq!(commit.signatures().count(), 1);
    // gpgsig is stored in headers; it is the only extra header here.
    assert_eq!(commit.headers().count(), 1);
    assert_eq!(commit.message(), "Signed commit");
}

#[test]
fn commit_gpgsig_is_preserved_and_strip_removes_it() {
    // Parsing preserves gpgsig so callers can extract it before stripping.
    let raw = r#"tree abc123
author Alice Liddell <alice@example.com> 1700000000 +0000
committer Alice Liddell <alice@example.com> 1700000000 +0000
gpgsig -----BEGIN SSH SIGNATURE-----
 AAAA==
 -----END SSH SIGNATURE-----

Signed commit"#;

    let commit = parse::<String, String>(raw).unwrap();
    assert_eq!(commit.signatures().count(), 1);

    let stripped = commit.strip_signatures();
    assert_eq!(stripped.signatures().count(), 0);
    assert_eq!(stripped.headers().count(), 0);
    assert_eq!(stripped.message(), "Signed commit");
}

#[test]
fn commit_with_trailers() {
    // The last paragraph contains only valid Token: value lines, so they
    // are split out into the trailers vec and excluded from the message.
    let raw = r#"tree abc123
author Alice Liddell <alice@example.com> 1700000000 +0000
committer Bob Bobson <bob@example.com> 1700000001 +0100

Add a new feature

This commit adds a new feature to the library.

Signed-off-by: Alice Liddell <alice@example.com>
Co-authored-by: Bob Bobson <bob@example.com>"#;

    let commit = parse::<String, String>(raw).unwrap();

    assert_eq!(
        commit.message(),
        "Add a new feature\n\nThis commit adds a new feature to the library."
    );
    let trailers: Vec<_> = commit.trailers().collect();
    assert_eq!(trailers.len(), 2);
    assert_eq!(&*trailers[0].token, "Signed-off-by");
    assert_eq!(trailers[0].value, "Alice Liddell <alice@example.com>");
    assert_eq!(&*trailers[1].token, "Co-authored-by");
    assert_eq!(trailers[1].value, "Bob Bobson <bob@example.com>");
}

#[test]
fn commit_last_paragraph_kept_in_message_when_not_all_trailers() {
    // If any line in the last paragraph is not a valid Token: value pair,
    // the entire paragraph stays in the message and no trailers are extracted.
    let raw = r#"tree abc123
author Alice Liddell <alice@example.com> 1700000000 +0000
committer Alice Liddell <alice@example.com> 1700000000 +0000

Add feature

Signed-off-by: Alice Liddell <alice@example.com>
This line is not a valid trailer."#;

    let commit = parse::<String, String>(raw).unwrap();

    assert_eq!(commit.trailers().count(), 0);
    assert!(commit.message().contains("Signed-off-by"));
    assert!(
        commit
            .message()
            .contains("This line is not a valid trailer.")
    );
}

#[test]
fn commit_with_extra_headers() {
    let raw = r#"tree abc123
author Alice Liddell <alice@example.com> 1700000000 +0000
committer Alice Liddell <alice@example.com> 1700000000 +0000
encoding UTF-8
mergetag some-value

Commit with extra headers"#;

    let commit = parse::<String, String>(raw).unwrap();

    let headers: Vec<_> = commit.headers().collect();
    assert_eq!(headers.len(), 2);
    assert_eq!(headers[0], ("encoding", "UTF-8"));
    assert_eq!(headers[1], ("mergetag", "some-value"));
}

#[test]
fn roundtrip() {
    // Parsing and then re-displaying a commit must produce output that parses
    // back to a CommitData equal in every field, exercising the Display /
    // parse_body symmetry in particular.
    let raw = r#"tree abc123
parent def456
author Alice Liddell <alice@example.com> 1700000000 +0000
committer Bob Bobson <bob@example.com> 1700000001 +0100

Add something useful

Signed-off-by: Alice Liddell <alice@example.com>"#;

    let commit = parse::<String, String>(raw).unwrap();
    let displayed = commit.to_string();
    let reparsed = parse::<String, String>(&displayed).unwrap();

    assert_eq!(commit.tree(), reparsed.tree());
    assert_eq!(
        commit.parents().collect::<Vec<_>>(),
        reparsed.parents().collect::<Vec<_>>()
    );
    assert_eq!(commit.author(), reparsed.author());
    assert_eq!(commit.committer(), reparsed.committer());
    assert_eq!(commit.message(), reparsed.message());
    assert_eq!(
        commit.trailers().collect::<Vec<_>>(),
        reparsed.trailers().collect::<Vec<_>>()
    );
}
