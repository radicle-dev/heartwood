mod helper;
mod property;

use std::collections::BTreeMap;

use nonempty::nonempty;

use crate::Storage;
use crate::crypto::{Signer as _, SigningKey};
use crate::git;
use crate::git::fmt::qualified_pattern;
use crate::identity::Visibility;
use crate::identity::doc::Doc;
use crate::rad;
use crate::storage::refs::{IDENTITY_BRANCH, IDENTITY_ROOT, SIGREFS_BRANCH, SIGREFS_PARENT};
use crate::storage::{ReadStorage, git::transport};
use crate::test::{arbitrary, fixtures};

use super::*;

#[test]
fn roundtrip() {
    use helper::*;

    let rule1 = Rule::new(Allowed::Delegates, 1);
    let rule2 = Rule::new(Allowed::Delegates, 1);
    let rule3 = Rule::new(Allowed::Delegates, 1);
    let mut rule4 = Rule::new(
        Allowed::Set(nonempty![
            did("did:key:z6MkpQTLwr8QyADGmBGAMsGttvWzP4PojUMs4hREZW5T5E3K"),
            did("did:key:z6MknG1nYDftMYUQ7eTBSGgqB2PL1xK5Pif33J3sRym3e8ye"),
        ]),
        2,
    );
    rule4.add_extensions(
        serde_json::json!({
            "foo": "bar",
            "quux": 5,
        })
        .as_object()
        .cloned()
        .unwrap(),
    );
    roundtrip(&rule1);
    roundtrip(&rule2);
    roundtrip(&rule3);
    roundtrip(&rule4);
}

#[test]
fn deserialization() {
    use helper::*;

    let examples = r#"
{
  "refs/heads/main": {
    "threshold": 2,
    "allow": [
      "did:key:z6MkpQTLwr8QyADGmBGAMsGttvWzP4PojUMs4hREZW5T5E3K",
      "did:key:z6MknG1nYDftMYUQ7eTBSGgqB2PL1xK5Pif33J3sRym3e8ye"
    ]
  },
  "refs/tags/releases/*": {
    "threshold": 2,
    "allow": [
      "did:key:z6MknLWe8A7UJxvTfY36JcB8XrP1KTLb5HFTX38hEmdY3b56",
      "did:key:z6Mkq2E5Se5H9gk1DsL1EMwR2t4CqSg3GFkNN2UeG4FNqXoP",
      "did:key:z6MkqRmXW5fbP9hJ1Y8j2N4CgVdJ2XJ6TsyXYf3FQ2NJgXax"
    ]
  },
  "refs/heads/development": {
    "threshold": 1,
    "allow": [
      "did:key:z6MkhH7ENYE62JAjTiRZPU71MGZ6xCwnbyHHWfrBu3fr6PVG"
    ]
  },
  "refs/heads/release/*": {
    "threshold": 1,
    "allow": "delegates"
  }
}
 "#;
    let expected = [
        (
            raw_pattern(qualified_pattern!("refs/heads/main")),
            Rule::new(
                Allowed::Set(nonempty![
                    did("did:key:z6MkpQTLwr8QyADGmBGAMsGttvWzP4PojUMs4hREZW5T5E3K"),
                    did("did:key:z6MknG1nYDftMYUQ7eTBSGgqB2PL1xK5Pif33J3sRym3e8ye"),
                ]),
                2,
            ),
        ),
        (
            raw_pattern(qualified_pattern!("refs/tags/releases/*")),
            Rule::new(
                Allowed::Set(nonempty![
                    did("did:key:z6MknLWe8A7UJxvTfY36JcB8XrP1KTLb5HFTX38hEmdY3b56"),
                    did("did:key:z6Mkq2E5Se5H9gk1DsL1EMwR2t4CqSg3GFkNN2UeG4FNqXoP"),
                    did("did:key:z6MkqRmXW5fbP9hJ1Y8j2N4CgVdJ2XJ6TsyXYf3FQ2NJgXax")
                ]),
                2,
            ),
        ),
        (
            raw_pattern(qualified_pattern!("refs/heads/development")),
            Rule::new(
                Allowed::Set(nonempty![did(
                    "did:key:z6MkhH7ENYE62JAjTiRZPU71MGZ6xCwnbyHHWfrBu3fr6PVG"
                )]),
                1,
            ),
        ),
        (
            raw_pattern(qualified_pattern!("refs/heads/release/*")),
            Rule::new(Allowed::Delegates, 1),
        ),
    ]
    .into_iter()
    .collect::<RawRules>();
    let rules = serde_json::from_str::<BTreeMap<RawPattern, RawRule>>(examples)
        .unwrap()
        .into();
    assert_eq!(expected, rules)
}

#[test]
fn ordering() {
    use helper::*;

    assert!(
        pattern(qualified_pattern!("refs/heads/a/b/c/d/*"))
            < pattern(qualified_pattern!("refs/heads/*/x")),
        "example 1"
    );
    assert!(
        pattern(qualified_pattern!("refs/heads/a")) < pattern(qualified_pattern!("refs/heads/*")),
        "example 2.a"
    );
    assert!(
        pattern(qualified_pattern!("refs/heads/abc"))
            < pattern(qualified_pattern!("refs/heads/a*")),
        "example 2.a"
    );
    assert!(
        pattern(qualified_pattern!("refs/heads/a/b/*"))
            < pattern(qualified_pattern!("refs/heads/a/*/c")),
        "example 2.a"
    );
    assert!(
        pattern(qualified_pattern!("refs/heads/aa*"))
            < pattern(qualified_pattern!("refs/heads/a*")),
        "example 2.b.A"
    );
    assert!(
        pattern(qualified_pattern!("refs/heads/a*b"))
            < pattern(qualified_pattern!("refs/heads/a*")),
        "example 2.b.B"
    );

    let pattern01 = pattern(qualified_pattern!("refs/tags/*"));
    let pattern02 = pattern(qualified_pattern!("refs/tags/v1"));
    let pattern04 = pattern(qualified_pattern!("refs/tags/v1.0.0"));
    let pattern05 = pattern(qualified_pattern!("refs/tags/release/v1.0.0"));
    let pattern03 = pattern(qualified_pattern!("refs/heads/main"));
    let pattern06 = pattern(qualified_pattern!("refs/tags/*/v1.0.0"));

    let pattern07 = pattern(qualified_pattern!("refs/tags/x*"));
    let pattern08 = pattern(qualified_pattern!("refs/tags/xx*"));

    let pattern09 = pattern(qualified_pattern!("refs/foos/*"));

    let pattern10 = pattern(qualified_pattern!("refs/heads/a"));
    let pattern11 = pattern(qualified_pattern!("refs/heads/b"));

    let pattern12 = pattern(qualified_pattern!("refs/heads/a/*"));
    let pattern13 = pattern(qualified_pattern!("refs/heads/b/*"));

    let pattern14 = pattern(qualified_pattern!("refs/heads/a/*/ab"));
    let pattern15 = pattern(qualified_pattern!("refs/heads/a/*/a"));

    let pattern16 = pattern(qualified_pattern!("refs/heads/a/*/b"));
    let pattern17 = pattern(qualified_pattern!("refs/heads/a/*/a"));

    // Test priority for path specificity
    assert!(
        pattern06 < pattern02,
        "match for 06 is always more specific since it has more components"
    );
    assert!(pattern02 < pattern01, "match for 02 is also match for 01");
    assert!(pattern08 < pattern07, "match for 08 is also match for 07");
    // Test equality
    assert!(pattern02 == pattern02);
    // Test lexicographical fallback when paths are equally specific
    assert!(pattern02 < pattern04);
    assert!(pattern03 < pattern01);
    assert!(pattern09 < pattern01);
    assert!(pattern10 < pattern11);
    assert!(pattern12 < pattern13);
    assert!(pattern15 < pattern14);
    assert!(
        pattern17 < pattern16,
        "matches have same length, but lexicographically, 'a' < 'b'"
    );

    // Test example from docs
    let pattern18 = pattern(qualified_pattern!("refs/tags/release/candidates/*"));
    let pattern19 = pattern(qualified_pattern!("refs/tags/release/*"));
    let pattern20 = pattern(qualified_pattern!("refs/tags/*"));

    assert!(pattern18 < pattern19);
    assert!(pattern19 < pattern20);

    let pattern21 = pattern(qualified_pattern!("refs/heads/dev"));

    assert!(pattern21 < pattern03);

    let mut patterns = [
        pattern01.clone(),
        pattern02.clone(),
        pattern03.clone(),
        pattern04.clone(),
        pattern05.clone(),
        pattern06.clone(),
    ];
    patterns.sort();

    assert_eq!(
        patterns,
        [
            pattern05, pattern06, pattern03, pattern02, pattern04, pattern01
        ]
    );
}

#[test]
fn deserialize_extensions() {
    let example = r#"
{
  "threshold": 2,
  "allow": [
    "did:key:z6MkpQTLwr8QyADGmBGAMsGttvWzP4PojUMs4hREZW5T5E3K",
    "did:key:z6MknG1nYDftMYUQ7eTBSGgqB2PL1xK5Pif33J3sRym3e8ye"
  ],
  "foo": "bar",
  "quux": 5
}
"#;
    let rule = serde_json::from_str::<Rule<Allowed, usize>>(example).unwrap();
    assert!(!rule.extensions().is_empty());
    let extensions = rule.extensions();
    assert_eq!(
        extensions.get("foo"),
        Some(serde_json::Value::String("bar".to_string())).as_ref()
    );
    assert_eq!(
        extensions.get("quux"),
        Some(serde_json::Value::Number(5.into())).as_ref()
    );
}

#[test]
fn rule_validate_success() {
    use helper::*;

    let doc = arbitrary::r#gen::<Doc>(1);
    let delegates = Allowed::Set(doc.delegates().as_ref().clone());
    let threshold = doc.majority();

    let rule = Rule::new(delegates, threshold);
    let result = rule.validate(&mut || resolve_from_doc(&doc));
    assert!(result.is_ok(), "failed to validate doc: {result:?}");

    let rule = Rule::new(Allowed::Delegates, 1);
    let result = rule.validate(&mut || resolve_from_doc(&doc));
    assert!(result.is_ok(), "failed to validate doc: {result:?}");
}

#[test]
fn rule_validate_failures() {
    use helper::*;

    let doc = arbitrary::r#gen::<Doc>(1);
    let pattern = pattern(qualified_pattern!("refs/heads/main"));

    assert!(matches!(
        Rule::new(Allowed::Delegates, 256).validate(&mut || resolve_from_doc(&doc)),
        Err(ValidationError::Threshold(_))
    ));

    let threshold = doc.delegates().len().saturating_add(1);
    assert!(matches!(
        Rule::new(Allowed::Delegates, threshold).validate(&mut || resolve_from_doc(&doc)),
        Err(ValidationError::Threshold(_))
    ));

    let delegates = NonEmpty::from_vec(
        arbitrary::set::<Did>(256..=256)
            .into_iter()
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert_eq!(delegates.len(), 256);
    assert!(matches!(
        Rule::new(delegates.into(), 1).validate(&mut || resolve_from_doc(&doc)),
        Err(ValidationError::Delegates(_))
    ));

    let delegate = Did::from(SigningKey::mock(22).public_key());

    let delegates = nonempty![delegate, delegate];
    assert_eq!(delegates.len(), 2);

    let expected = Rule {
        allow: ResolvedDelegates::Set(doc::Delegates::new(nonempty![delegate]).unwrap()),
        threshold: doc::Threshold::MIN,
        extensions: json::Map::new(),
    };
    assert_eq!(
        Rule::new(delegates.into(), 1)
            .validate(&mut || resolve_from_doc(&doc))
            .unwrap(),
        expected,
    );

    // Duplicate rules are overwritten
    let rules = vec![
        (
            pattern.clone().into_inner(),
            Rule::new(Allowed::Delegates, 1),
        ),
        (
            pattern.clone().into_inner(),
            Rule::new(doc.delegates().as_ref().clone().into(), 1),
        ),
    ];
    let expected = [(
        pattern,
        Rule::new(
            ResolvedDelegates::Set(doc.delegates().clone()),
            doc::Threshold::MIN,
        ),
    )]
    .into_iter()
    .collect::<Rules>();
    assert_eq!(
        Rules::from_raw(rules, &mut || resolve_from_doc(&doc)).unwrap(),
        expected
    );
}

#[test]
fn canonical() {
    use helper::*;

    let tempdir = tempfile::tempdir().unwrap();
    let storage = Storage::open(tempdir.path().join("storage"), fixtures::user()).unwrap();

    transport::local::register(storage.clone());

    let delegate = SigningKey::mock(0xff);
    let contributor = SigningKey::mock(0xfe);
    let (repo, head) = fixtures::repository(tempdir.path().join("working"));
    let (rid, doc, _) = rad::init(
        &repo,
        "heartwood".try_into().unwrap(),
        "Radicle Heartwood Protocol & Stack",
        git::fmt::refname!("master"),
        Visibility::default(),
        &delegate,
        &storage,
    )
    .unwrap();

    let mut doc = doc.edit();
    // Ensure there is a second delegate for testing overlapping rules
    doc.delegate(contributor.public_key().into());

    // Create tags and keep track of their OIDs
    //
    // follows the `refs/tags/release/candidates/*` rule
    let failing_tag = git::fmt::refname!("release/candidates/v1.0");
    let tags = [
        // follows the `refs/tags/*` rule
        git::fmt::refname!("v1.0"),
        // follows the `refs/tags/release/*` rule
        git::fmt::refname!("release/v1.0"),
        failing_tag.clone(),
        // follows the `refs/tags/*` rule
        git::fmt::refname!("qa/v1.0"),
    ]
    .into_iter()
    .map(|name| {
        (
            git::fmt::lit::refs_tags(name.clone()).into(),
            tag(name, head, &repo),
        )
    })
    .collect::<BTreeMap<Qualified, _>>();

    git::push(
        &repo,
        &rad::REMOTE_NAME,
        [
            (
                &git::fmt::qualified!("refs/tags/v1.0"),
                &git::fmt::qualified!("refs/tags/v1.0"),
            ),
            (
                &git::fmt::qualified!("refs/tags/release/v1.0"),
                &git::fmt::qualified!("refs/tags/release/v1.0"),
            ),
            (
                &git::fmt::qualified!("refs/tags/release/candidates/v1.0"),
                &git::fmt::qualified!("refs/tags/release/candidates/v1.0"),
            ),
            (
                &git::fmt::qualified!("refs/tags/qa/v1.0"),
                &git::fmt::qualified!("refs/tags/qa/v1.0"),
            ),
        ],
    )
    .unwrap();

    let rules = Rules::from_raw(
        [
            (
                raw_pattern(qualified_pattern!("refs/tags/*")),
                Rule::new(Allowed::Delegates, 1),
            ),
            (
                raw_pattern(qualified_pattern!("refs/tags/release/*")),
                Rule::new(Allowed::Delegates, 1),
            ),
            // Ensure that none of the other rules apply by ensuring we need
            // both delegates to get the quorum of the
            // `refs/tags/release/candidates/v1.0` reference
            (
                raw_pattern(qualified_pattern!("refs/tags/release/candidates/*")),
                Rule::new(Allowed::Delegates, 2),
            ),
        ],
        &mut || resolve_from_doc(&doc.clone().verified().unwrap()),
    )
    .unwrap();

    // All tags should succeed at getting their canonical commit other than the
    // candidates tag.
    let stored = storage.repository(rid).unwrap();
    let failing = git::fmt::Qualified::from(git::fmt::lit::refs_tags(failing_tag));
    for (refname, oid) in tags.into_iter() {
        let canonical = rules
            .canonical(refname.clone(), &stored)
            .unwrap_or_else(|| {
                panic!("there should be a matching rule for {refname}, rules: {rules:#?}")
            });
        if refname == failing {
            assert!(canonical.find_objects().unwrap().quorum().is_err());
        } else {
            assert_eq!(
                canonical
                    .find_objects()
                    .unwrap()
                    .quorum()
                    .unwrap_or_else(|e| panic!("quorum error for {refname}: {e}")),
                canonical::Quorum {
                    refname,
                    object: canonical::Object::Tag { id: oid },
                }
            )
        }
    }
}

#[test]
fn special_branches() {
    assert!(Pattern::new((*IDENTITY_BRANCH).clone().into()).is_err());
    assert!(Pattern::new((*SIGREFS_BRANCH).clone().into()).is_err());
    assert!(Pattern::new((*SIGREFS_PARENT).clone().into()).is_err());
    assert!(Pattern::new((*IDENTITY_ROOT).clone().into()).is_err());
}

#[test]
fn matches_expands_globs_appropriately() {
    let exact = qualified_pattern!("refs/heads/main");
    assert!(matches(&exact, &git::fmt::qualified!("refs/heads/main")));
    assert!(!matches(&exact, &git::fmt::qualified!("refs/heads/main-2")));
    assert!(!matches(&exact, &git::fmt::qualified!("refs/heads/other")));

    let trailing_slash = qualified_pattern!("refs/heads/*");
    assert!(matches(
        &trailing_slash,
        &git::fmt::qualified!("refs/heads/main")
    ));
    assert!(matches(
        &trailing_slash,
        &git::fmt::qualified!("refs/heads/feature/1")
    ));
    assert!(!matches(
        &trailing_slash,
        &git::fmt::qualified!("refs/tags/main")
    ));

    let trailing_text = qualified_pattern!("refs/heads/feature-*");
    assert!(matches(
        &trailing_text,
        &git::fmt::qualified!("refs/heads/feature-")
    ));
    assert!(matches(
        &trailing_text,
        &git::fmt::qualified!("refs/heads/feature-1")
    ));
    assert!(matches(
        &trailing_text,
        &git::fmt::qualified!("refs/heads/feature-1/sub")
    ));
    assert!(!matches(
        &trailing_text,
        &git::fmt::qualified!("refs/heads/feature")
    ));

    // Because `*` expands to `**`, it matches zero or more path components.
    let middle = qualified_pattern!("refs/heads/*/main");
    assert!(matches(
        &middle,
        &git::fmt::qualified!("refs/heads/alice/main")
    ));
    assert!(matches(
        &middle,
        &git::fmt::qualified!("refs/heads/alice/bob/main")
    ));

    //
    // NOTE(Ade): In git this won't match as glob '*' isn't inclusive of zero matches like `**`
    // See: https://git.kernel.org/pub/scm/git/git.git/tree/refspec.c?h=v2.54.0&id=94f057755b7941b321fd11fec1b2e3ca5313a4e0#n298
    //
    assert!(matches(&middle, &git::fmt::qualified!("refs/heads/main")));

    assert!(!matches(
        &middle,
        &git::fmt::qualified!("refs/heads/alice/dev")
    ));

    // HardenedBSD glob expansion issue
    let hbsd = qualified_pattern!("refs/heads/quarterly/hardened/15-stable/main*");
    assert!(matches(
        &hbsd,
        &git::fmt::qualified!("refs/heads/quarterly/hardened/15-stable/main-2026q2")
    ));
    assert!(matches(
        &hbsd,
        &git::fmt::qualified!("refs/heads/quarterly/hardened/15-stable/main/2026q2")
    ));
}

#[test]
fn matches_exactly_curly_braces() {
    let exact = qualified_pattern!("refs/heads/{foo,bar}");
    assert!(matches(
        &exact,
        &git::fmt::qualified!("refs/heads/{foo,bar}")
    ));
    assert!(!matches(&exact, &git::fmt::qualified!("refs/heads/foo")));
    assert!(!matches(&exact, &git::fmt::qualified!("refs/heads/bar")));
}
