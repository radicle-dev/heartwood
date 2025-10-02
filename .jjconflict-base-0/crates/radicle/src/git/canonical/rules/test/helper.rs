use radicle_git_ref_format::refspec::QualifiedPattern;

use crate::git;
use crate::git::canonical::rules::{Allowed, Pattern, RawPattern, Rule};
use crate::git::fmt::RefString;
use crate::identity::doc::{self, Doc};
use crate::prelude::Did;
use crate::test::fixtures;

pub fn roundtrip(rule: &Rule<Allowed, usize>) {
    let json = serde_json::to_string(rule).unwrap();
    assert_eq!(
        *rule,
        serde_json::from_str(&json).unwrap(),
        "failed to roundtrip: {json}"
    )
}

pub fn did(s: &str) -> Did {
    s.parse().unwrap()
}

pub fn raw_pattern(qp: QualifiedPattern<'static>) -> RawPattern {
    qp
}

pub fn pattern(qp: QualifiedPattern<'static>) -> Pattern {
    Pattern::new(qp).unwrap()
}

pub fn resolve_from_doc(doc: &Doc) -> doc::Delegates {
    doc.delegates().clone()
}

pub fn tag(name: RefString, head: git::raw::Oid, repo: &git::raw::Repository) -> git::Oid {
    let commit = fixtures::commit(name.as_str(), &[head], repo);
    let target = repo.find_object(commit.into(), None).unwrap();
    let tagger = repo.signature().unwrap();
    repo.tag(name.as_str(), &target, &tagger, name.as_str(), false)
        .unwrap()
        .into()
}
