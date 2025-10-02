use std::fmt;

use qcheck::{Arbitrary, TestResult};
use qcheck_macros::quickcheck;

use crate::git;
use crate::git::canonical::rules::{RawPattern, matches};

/// Newtype wrapper around [`git::fmt::Component`].
///
/// It implements [`qcheck::Arbitrary`] given the rules found at
/// <https://git-scm.com/docs/git-check-ref-format>.
///
/// The implemented rules are:
/// * They can include slash / for hierarchical (directory) grouping, but no
///   slash-separated component can begin with a dot '.' or end with the sequence
///   '.lock'.
/// * They must contain at least one '/'. This enforces the presence of a category
///   like 'heads/', 'tags/' etc. but the actual names are not restricted. If the
///   --allow-onelevel option is used, this rule is waived.
/// * They cannot have two consecutive dots '..' anywhere.
/// * They cannot have ASCII control characters (i.e. bytes whose values are
///   lower than \040, or \177 DEL), space, tilde '~', caret '^', or colon ':'
///   anywhere.
/// * They cannot have question-mark '?', asterisk '*', or open bracket '['
///   anywhere. See the --refspec-pattern option below for an exception to this
///   rule.
/// * They cannot begin or end with a slash '/' or contain multiple consecutive
///   slashes (see the --normalize option below for an exception to this rule).
/// * They cannot end with a dot '..'
/// * They cannot contain a sequence '@{'.
/// * They cannot be the single character '@'.
/// * They cannot contain a '\'.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Component {
    inner: String,
}

impl fmt::Display for Component {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.inner.as_str())
    }
}

impl Component {
    fn make_valid_comp(s: String) -> String {
        let mut comp: String = s
            .chars()
            .filter(|&c| {
                let is_control_or_space = c <= ' ' || c == '\x7F';
                let is_forbidden_sym = ['~', '^', ':', '?', '*', '[', '\\', '/'].contains(&c);
                !is_control_or_space && !is_forbidden_sym
            })
            .collect();

        while comp.contains("..") {
            comp = comp.replace("..", ".");
        }
        while comp.contains("@{") {
            comp = comp.replace("@{", "@");
        }

        if comp.starts_with('.') {
            comp.insert(0, 'a');
        }
        if comp.ends_with(".lock") {
            comp.push('a');
        }
        if comp.ends_with('.') {
            comp.push('a');
        }
        if comp == "@" {
            comp.push('a');
        }

        if comp.is_empty() {
            comp.push('a');
        }

        comp
    }
}

impl Arbitrary for Component {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let component = Self::make_valid_comp(String::arbitrary(g));
        Self { inner: component }
    }
}

fn to_refname(name: &str) -> Option<git::fmt::Qualified<'_>> {
    let refstr = git::fmt::RefStr::try_from_str(name).ok()?;
    git::fmt::Qualified::from_refstr(refstr)
}

fn parse_pattern(pat: &str) -> Option<RawPattern> {
    serde_json::from_value(serde_json::Value::String(pat.to_string())).ok()
}

#[quickcheck]
fn identity(c1: Component, c2: Component, c3: Component) -> TestResult {
    let name = format!("refs/{c1}/{c2}/{c3}");

    let refname = match to_refname(&name) {
        Some(p) => p,
        None => return TestResult::discard(),
    };
    let pattern = match parse_pattern(&name) {
        Some(p) => p,
        None => return TestResult::discard(),
    };

    TestResult::from_bool(matches(&pattern, &refname))
}

#[quickcheck]
fn prefix(c1: Component, c2: Component, c3: Component) -> TestResult {
    let pattern = match parse_pattern(&format!("refs/{c1}/*")) {
        Some(p) => p,
        None => return TestResult::discard(),
    };

    let cases = [
        format!("refs/{c1}/{c2}/{c3}"),
        format!("refs/{c1}/{c2}"),
        format!("refs/{c1}/{c3}"),
        format!("refs/{c1}/{c3}/{c2}"),
    ];

    let refnames: Option<Vec<_>> = cases.iter().map(|n| to_refname(n)).collect();

    match refnames {
        None => TestResult::discard(),
        Some(refs) => TestResult::from_bool(refs.iter().all(|r| matches(&pattern, r))),
    }
}

#[quickcheck]
fn suffix(c1: Component, c2: Component, c3: Component) -> TestResult {
    let pattern = match parse_pattern(&format!("refs/*/{c3}")) {
        Some(p) => p,
        None => return TestResult::discard(),
    };

    let cases = [
        format!("refs/{c1}/{c2}/{c3}"),
        format!("refs/a/{c3}"),
        format!("refs/{c2}/{c3}"),
        format!("refs/{c1}/{c3}"),
        format!("refs/{c2}/{c1}/{c3}"),
    ];

    let refnames: Option<Vec<_>> = cases.iter().map(|n| to_refname(n)).collect();

    match refnames {
        None => TestResult::discard(),
        Some(refs) => TestResult::from_bool(refs.iter().all(|r| matches(&pattern, r))),
    }
}

#[quickcheck]
fn trailing_asterisk_partial_component(c1: Component, c2: Component, c3: Component) -> TestResult {
    let pattern = match parse_pattern(&format!("refs/{c1}/{c2}*")) {
        Some(p) => p,
        None => return TestResult::discard(),
    };

    let cases = [
        format!("refs/{c1}/{c2}{c3}"),
        format!("refs/{c1}/{c2}-{c3}"),
        format!("refs/{c1}/{c2}/{c3}"),
    ];

    let refnames: Option<Vec<_>> = cases.iter().map(|n| to_refname(n)).collect();

    match refnames {
        None => TestResult::discard(),
        Some(refs) => TestResult::from_bool(refs.iter().all(|r| matches(&pattern, r))),
    }
}

#[quickcheck]
fn prefix_negative(c1: Component, c2: Component, c3: Component) -> TestResult {
    if c1 == c2 || c1 == c3 {
        return TestResult::discard();
    }

    let pattern = match parse_pattern(&format!("refs/{c1}/*")) {
        Some(p) => p,
        None => return TestResult::discard(),
    };

    let cases = [
        format!("refs/{c2}/{c3}"),
        format!("refs/{c3}/{c2}"),
        format!("refs/{c2}/a"),
        format!("refs/{c3}/a"),
    ];

    let refnames: Option<Vec<_>> = cases.iter().map(|n| to_refname(n)).collect();

    match refnames {
        None => TestResult::discard(),
        Some(refs) => TestResult::from_bool(refs.iter().all(|r| !matches(&pattern, r))),
    }
}

#[quickcheck]
fn suffix_negative(c1: Component, c2: Component, c3: Component) -> TestResult {
    if c3 == c1 || c3 == c2 {
        return TestResult::discard();
    }

    let pattern = match parse_pattern(&format!("refs/*/{c3}")) {
        Some(p) => p,
        None => return TestResult::discard(),
    };

    let cases = [
        format!("refs/{c1}/{c2}"),
        format!("refs/{c2}/{c1}"),
        format!("refs/a/{c1}"),
        format!("refs/a/{c2}"),
    ];

    let refnames: Option<Vec<_>> = cases.iter().map(|n| to_refname(n)).collect();

    match refnames {
        None => TestResult::discard(),
        Some(refs) => TestResult::from_bool(refs.iter().all(|r| !matches(&pattern, r))),
    }
}
