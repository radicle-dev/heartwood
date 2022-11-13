use std::ops::ControlFlow;

use crypto::test::signer::MockSigner;
use quickcheck::Arbitrary;
use radicle_crypto::Signer;

use crate::{create, get, history, list, update, Create, ObjectId, TypeName, Update};

use super::test;

#[test]
fn roundtrip() {
    let storage = test::Storage::new();
    let signer = gen::<MockSigner>(1);
    let terry = test::Person::new(&storage, "terry", *signer.public_key()).unwrap();
    let proj = test::Project::new(&storage, "discworld", *signer.public_key()).unwrap();
    let proj = test::RemoteProject {
        project: proj,
        person: terry.clone(),
    };
    let contents = history::Contents::Automerge(Vec::new());
    let typename = "xyz.rad.issue".parse::<TypeName>().unwrap();
    let cob = create(
        &storage,
        &signer,
        &proj,
        &proj.identifier(),
        Create {
            author: Some(terry),
            contents,
            typename: typename.clone(),
            message: "creating xyz.rad.issue".to_string(),
        },
    )
    .unwrap();

    let expected = get(&storage, &typename, cob.id())
        .unwrap()
        .expect("BUG: cob was missing");

    assert_eq!(cob, expected);
}

#[test]
fn list_cobs() {
    let storage = test::Storage::new();
    let signer = gen::<MockSigner>(1);
    let terry = test::Person::new(&storage, "terry", *signer.public_key()).unwrap();
    let proj = test::Project::new(&storage, "discworld", *signer.public_key()).unwrap();
    let proj = test::RemoteProject {
        project: proj,
        person: terry.clone(),
    };
    let typename = "xyz.rad.issue".parse::<TypeName>().unwrap();
    let issue_1 = create(
        &storage,
        &signer,
        &proj,
        &proj.identifier(),
        Create {
            author: Some(terry.clone()),
            contents: history::Contents::Automerge(b"issue 1".to_vec()),
            typename: typename.clone(),
            message: "creating xyz.rad.issue".to_string(),
        },
    )
    .unwrap();

    let issue_2 = create(
        &storage,
        &signer,
        &proj,
        &proj.identifier(),
        Create {
            author: Some(terry),
            contents: history::Contents::Automerge(b"issue 2".to_vec()),
            typename: typename.clone(),
            message: "commenting xyz.rad.issue".to_string(),
        },
    )
    .unwrap();

    let mut expected = list(&storage, &typename).unwrap();
    expected.sort_by(|x, y| x.id().cmp(y.id()));

    let mut actual = vec![issue_1, issue_2];
    actual.sort_by(|x, y| x.id().cmp(y.id()));

    assert_eq!(actual, expected);
}

#[test]
fn update_cob() {
    let storage = test::Storage::new();
    let signer = gen::<MockSigner>(1);
    let terry = test::Person::new(&storage, "terry", *signer.public_key()).unwrap();
    let proj = test::Project::new(&storage, "discworld", *signer.public_key()).unwrap();
    let proj = test::RemoteProject {
        project: proj,
        person: terry.clone(),
    };
    let contents = history::Contents::Automerge(Vec::new());
    let typename = "xyz.rad.issue".parse::<TypeName>().unwrap();
    let cob = create(
        &storage,
        &signer,
        &proj,
        &proj.identifier(),
        Create {
            author: Some(terry.clone()),
            contents,
            typename: typename.clone(),
            message: "creating xyz.rad.issue".to_string(),
        },
    )
    .unwrap();

    let not_expected = get(&storage, &typename, cob.id())
        .unwrap()
        .expect("BUG: cob was missing");

    let updated = update(
        &storage,
        &signer,
        &proj,
        &proj.identifier(),
        Update {
            author: Some(terry),
            changes: history::Contents::Automerge(b"issue 1".to_vec()),
            object_id: *cob.id(),
            typename: typename.clone(),
            message: "commenting xyz.rad.issue".to_string(),
        },
    )
    .unwrap();

    let expected = get(&storage, &typename, updated.id())
        .unwrap()
        .expect("BUG: cob was missing");

    assert_ne!(updated, not_expected);
    assert_eq!(updated, expected);
}

#[test]
fn traverse_cobs() {
    let storage = test::Storage::new();
    let neil_signer = gen::<MockSigner>(2);
    let neil = test::Person::new(&storage, "gaiman", *neil_signer.public_key()).unwrap();
    let terry_signer = gen::<MockSigner>(1);
    let terry = test::Person::new(&storage, "pratchett", *terry_signer.public_key()).unwrap();
    let proj = test::Project::new(&storage, "discworld", *terry_signer.public_key()).unwrap();
    let terry_proj = test::RemoteProject {
        project: proj.clone(),
        person: terry.clone(),
    };
    let neil_proj = test::RemoteProject {
        project: proj,
        person: neil.clone(),
    };
    let typename = "xyz.rad.issue".parse::<TypeName>().unwrap();
    let cob = create(
        &storage,
        &terry_signer,
        &terry_proj,
        &terry_proj.identifier(),
        Create {
            author: Some(terry),
            contents: history::Contents::Automerge(b"issue 1".to_vec()),
            typename: typename.clone(),
            message: "creating xyz.rad.issue".to_string(),
        },
    )
    .unwrap();
    copy_to(
        storage.as_raw(),
        &terry_proj,
        &neil_proj,
        &typename,
        *cob.id(),
    )
    .unwrap();

    let updated = update(
        &storage,
        &neil_signer,
        &neil_proj,
        &neil_proj.identifier(),
        Update {
            author: Some(neil),
            changes: history::Contents::Automerge(b"issue 2".to_vec()),
            object_id: *cob.id(),
            typename,
            message: "commenting on xyz.rad.issue".to_string(),
        },
    )
    .unwrap();

    // traverse over the history and filter by changes that were only authorized by terry
    let contents = updated.history().traverse(Vec::new(), |mut acc, entry| {
        let author = match entry.author() {
            Some(author) => test::Person::find_by_oid(storage.as_raw(), *author).unwrap(),
            None => None,
        };
        let project = test::Project::find_by_oid(storage.as_raw(), entry.resource()).unwrap();

        if let (Some(author), Some(project)) = (author, project) {
            if project.delegate_check(&author) {
                acc.push(entry.contents().as_ref().to_vec())
            }
        }
        ControlFlow::Continue(acc)
    });

    assert_eq!(contents, vec![b"issue 1".to_vec()]);

    // traverse over the history and filter by changes that were only authorized by neil
    let contents = updated.history().traverse(Vec::new(), |mut acc, entry| {
        acc.push(entry.contents().as_ref().to_vec());
        ControlFlow::Continue(acc)
    });

    assert_eq!(contents, vec![b"issue 1".to_vec(), b"issue 2".to_vec()]);
}

fn gen<T: Arbitrary>(size: usize) -> T {
    let mut gen = quickcheck::Gen::new(size);

    T::arbitrary(&mut gen)
}

fn copy_to(
    repo: &git2::Repository,
    from: &test::RemoteProject,
    to: &test::RemoteProject,
    typename: &TypeName,
    object: ObjectId,
) -> Result<(), git2::Error> {
    let original = {
        let name = format!(
            "refs/rad/{}/cobs/{}/{}",
            from.identifier().to_path(),
            typename,
            object
        );
        let r = repo.find_reference(&name)?;
        r.target().unwrap()
    };

    let name = format!(
        "refs/rad/{}/cobs/{}/{}",
        to.identifier().to_path(),
        typename,
        object
    );
    repo.reference(&name, original, false, "copying object reference")?;
    Ok(())
}
