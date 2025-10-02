use std::sync::{Mutex, MutexGuard};

use radicle_git_ref_format::{component, refname};

use crate::{Branch, Error, Glob, Repository};

use super::platinum;

#[test]
fn basic_test() -> Result<(), Error> {
    let shared_repo = Mutex::new(Repository::open(platinum::get())?);
    let locked_repo: MutexGuard<Repository> = shared_repo.lock().unwrap();
    let mut branches = locked_repo
        .branches(Glob::all_heads().branches().and(Glob::all_remotes()))?
        .collect::<Result<Vec<_>, _>>()?;
    branches.sort();

    let origin = component!("origin");
    let banana = component!("banana");
    assert_eq!(
        branches,
        vec![
            Branch::local(refname!("dev")),
            Branch::local(refname!("diff-test")),
            Branch::local(refname!("empty-branch")),
            Branch::local(refname!("master")),
            Branch::remote(banana.clone(), refname!("orange/pineapple")),
            Branch::remote(banana, refname!("pineapple")),
            Branch::remote(origin.clone(), refname!("HEAD")),
            Branch::remote(origin.clone(), refname!("dev")),
            Branch::remote(origin.clone(), refname!("diff-test")),
            Branch::remote(origin.clone(), refname!("empty-branch")),
            Branch::remote(origin, refname!("master")),
        ]
    );

    Ok(())
}
