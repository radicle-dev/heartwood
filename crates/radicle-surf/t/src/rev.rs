use std::str::FromStr;

use radicle_git_ext::ref_format::{name::component, refname};
use radicle_surf::{Branch, Error, Oid, Repository};

use super::GIT_PLATINUM;

// **FIXME**: This seems to break occasionally on
// buildkite. For some reason the commit
// 3873745c8f6ffb45c990eb23b491d4b4b6182f95, which is on master
// (currently HEAD), is not found. It seems to load the history
// with d6880352fc7fda8f521ae9b7357668b17bb5bad5 as the HEAD.
//
// To temporarily fix this, we need to select "New Build" from the build kite
// build page that's failing.
// * Under "Message" put whatever you want.
// * Under "Branch" put in the branch you're working on.
// * Expand "Options" and select "clean checkout".
#[test]
fn _master() -> Result<(), Error> {
    let repo = Repository::open(GIT_PLATINUM)?;
    let mut history = repo.history(Branch::remote(component!("origin"), refname!("master")))?;

    let commit1 = Oid::from_str("3873745c8f6ffb45c990eb23b491d4b4b6182f95")?;
    assert!(
        history.any(|commit| commit.unwrap().id == commit1),
        "commit_id={}, history =\n{:#?}",
        commit1,
        &history
    );

    let commit2 = Oid::from_str("d6880352fc7fda8f521ae9b7357668b17bb5bad5")?;
    assert!(
        history.any(|commit| commit.unwrap().id == commit2),
        "commit_id={}, history =\n{:#?}",
        commit2,
        &history
    );

    Ok(())
}

#[test]
fn commit() -> Result<(), Error> {
    let repo = Repository::open(GIT_PLATINUM)?;
    let rev = Oid::from_str("3873745c8f6ffb45c990eb23b491d4b4b6182f95")?;
    let mut history = repo.history(rev)?;

    let commit1 = Oid::from_str("3873745c8f6ffb45c990eb23b491d4b4b6182f95")?;
    assert!(history.any(|commit| commit.unwrap().id == commit1));

    Ok(())
}

#[test]
fn commit_parents() -> Result<(), Error> {
    let repo = Repository::open(GIT_PLATINUM)?;
    let rev = Oid::from_str("3873745c8f6ffb45c990eb23b491d4b4b6182f95")?;
    let history = repo.history(rev)?;
    let commit = history.head();

    assert_eq!(
        commit.parents,
        vec![Oid::from_str("d6880352fc7fda8f521ae9b7357668b17bb5bad5")?]
    );

    Ok(())
}

#[test]
fn commit_short() -> Result<(), Error> {
    let repo = Repository::open(GIT_PLATINUM)?;
    let rev = repo.oid("3873745c8")?;
    let mut history = repo.history(rev)?;

    let commit1 = Oid::from_str("3873745c8f6ffb45c990eb23b491d4b4b6182f95")?;
    assert!(history.any(|commit| commit.unwrap().id == commit1));

    Ok(())
}

#[test]
fn tag() -> Result<(), Error> {
    let repo = Repository::open(GIT_PLATINUM)?;
    let rev = refname!("refs/tags/v0.2.0");
    let history = repo.history(&rev)?;

    let commit1 = Oid::from_str("2429f097664f9af0c5b7b389ab998b2199ffa977")?;
    assert_eq!(history.head().id, commit1);

    Ok(())
}
