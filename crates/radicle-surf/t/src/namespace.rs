use pretty_assertions::{assert_eq, assert_ne};
use radicle_git_ext::ref_format::{name::component, refname, refspec};
use radicle_surf::{Branch, Error, Glob, Repository};

use super::GIT_PLATINUM;

#[test]
fn switch_to_banana() -> Result<(), Error> {
    let repo = Repository::open(GIT_PLATINUM)?;
    let history_master = repo.history(Branch::local(refname!("master")))?;
    repo.switch_namespace(&refname!("golden"))?;
    let history_banana = repo.history(Branch::local(refname!("banana")))?;

    assert_ne!(history_master.head(), history_banana.head());

    Ok(())
}

#[test]
fn me_namespace() -> Result<(), Error> {
    let repo = Repository::open(GIT_PLATINUM)?;
    let history = repo.history(Branch::local(refname!("master")))?;

    assert_eq!(repo.which_namespace().unwrap(), None);

    repo.switch_namespace(&refname!("me"))?;
    assert_eq!(repo.which_namespace().unwrap(), Some("me".parse()?));

    let history_feature = repo.history(Branch::local(refname!("feature/#1194")))?;
    assert_eq!(history.head(), history_feature.head());

    let expected_branches: Vec<Branch> = vec![Branch::local(refname!("feature/#1194"))];
    let mut branches = repo
        .branches(Glob::all_heads())?
        .collect::<Result<Vec<_>, _>>()?;
    branches.sort();

    assert_eq!(expected_branches, branches);

    let expected_branches: Vec<Branch> = vec![Branch::remote(
        component!("fein"),
        refname!("heads/feature/#1194"),
    )];
    let mut branches = repo
        .branches(Glob::remotes(refspec::pattern!("fein/*")))?
        .collect::<Result<Vec<_>, _>>()?;
    branches.sort();

    assert_eq!(expected_branches, branches);

    Ok(())
}

#[test]
fn golden_namespace() -> Result<(), Error> {
    let repo = Repository::open(GIT_PLATINUM)?;
    let history = repo.history(Branch::local(refname!("master")))?;

    assert_eq!(repo.which_namespace().unwrap(), None);

    repo.switch_namespace(&refname!("golden"))?;

    assert_eq!(repo.which_namespace().unwrap(), Some("golden".parse()?));

    let golden_history = repo.history(Branch::local(refname!("master")))?;
    assert_eq!(history.head(), golden_history.head());

    let expected_branches: Vec<Branch> = vec![
        Branch::local(refname!("banana")),
        Branch::local(refname!("master")),
    ];
    let mut branches = repo
        .branches(Glob::all_heads())?
        .collect::<Result<Vec<_>, _>>()?;
    branches.sort();

    assert_eq!(expected_branches, branches);

    // NOTE: these tests used to remove the categories, i.e. heads & tags, but that
    // was specialised logic based on the radicle-link storage layout.
    let remote = component!("kickflip");
    let expected_branches: Vec<Branch> = vec![
        Branch::remote(remote.clone(), refname!("heads/fakie/bigspin")),
        Branch::remote(remote.clone(), refname!("heads/heelflip")),
        Branch::remote(remote, refname!("tags/v0.1.0")),
    ];
    let mut branches = repo
        .branches(Glob::remotes(refspec::pattern!("kickflip/*")))?
        .collect::<Result<Vec<_>, _>>()?;
    branches.sort();

    assert_eq!(expected_branches, branches);

    Ok(())
}

#[test]
fn silver_namespace() -> Result<(), Error> {
    let repo = Repository::open(GIT_PLATINUM)?;
    let history = repo.history(Branch::local(refname!("master")))?;

    assert_eq!(repo.which_namespace().unwrap(), None);

    repo.switch_namespace(&refname!("golden/silver"))?;
    assert_eq!(
        repo.which_namespace().unwrap(),
        Some("golden/silver".parse()?)
    );
    let silver_history = repo.history(Branch::local(refname!("master")))?;
    assert_ne!(history.head(), silver_history.head());

    let expected_branches: Vec<Branch> = vec![Branch::local(refname!("master"))];
    let mut branches = repo
        .branches(Glob::all_heads().branches().and(Glob::all_remotes()))?
        .collect::<Result<Vec<_>, _>>()?;
    branches.sort();

    assert_eq!(expected_branches, branches);

    Ok(())
}
