use std::ops::ControlFlow;

use crypto::test::signer::MockSigner;
use crypto::{PublicKey, Signer};
use git_ext::ref_format::{refname, Component, RefString};
use nonempty::{nonempty, NonEmpty};
use qcheck::Arbitrary;

use crate::{
    create, get, list, object, test::arbitrary::Invalid, update, Create, Entry, ObjectId, TypeName,
    Update, Updated, Version,
};

use super::test;

#[test]
fn roundtrip() {
    let storage = test::Storage::new();
    let signer = gen::<MockSigner>(1);
    let terry = test::Person::new(&storage, "terry", *signer.public_key()).unwrap();
    let proj = test::Project::new(&storage, "discworld", *signer.public_key()).unwrap();
    let proj = test::RemoteProject {
        project: proj,
        person: terry,
    };
    let typename = "xyz.rad.issue".parse::<TypeName>().unwrap();
    let cob = create::<NonEmpty<Entry>, _, _>(
        &storage,
        &signer,
        proj.project.content_id,
        vec![],
        signer.public_key(),
        Create {
            contents: nonempty!(Vec::new()),
            type_name: typename.clone(),
            message: "creating xyz.rad.issue".to_string(),
            embeds: vec![],
            version: Version::default(),
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
        person: terry,
    };
    let typename = "xyz.rad.issue".parse::<TypeName>().unwrap();
    let issue_1 = create::<NonEmpty<Entry>, _, _>(
        &storage,
        &signer,
        proj.project.content_id,
        vec![],
        signer.public_key(),
        Create {
            contents: nonempty!(b"issue 1".to_vec()),
            type_name: typename.clone(),
            message: "creating xyz.rad.issue".to_string(),
            embeds: vec![],
            version: Version::default(),
        },
    )
    .unwrap();

    let issue_2 = create(
        &storage,
        &signer,
        proj.project.content_id,
        vec![],
        signer.public_key(),
        Create {
            contents: nonempty!(b"issue 2".to_vec()),
            type_name: typename.clone(),
            message: "commenting xyz.rad.issue".to_string(),
            embeds: vec![],
            version: Version::default(),
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
        person: terry,
    };
    let typename = "xyz.rad.issue".parse::<TypeName>().unwrap();
    let cob = create::<NonEmpty<Entry>, _, _>(
        &storage,
        &signer,
        proj.project.content_id,
        vec![],
        signer.public_key(),
        Create {
            contents: nonempty!(Vec::new()),
            type_name: typename.clone(),
            message: "creating xyz.rad.issue".to_string(),
            embeds: vec![],
            version: Version::default(),
        },
    )
    .unwrap();

    let not_expected = get::<NonEmpty<Entry>, _>(&storage, &typename, cob.id())
        .unwrap()
        .expect("BUG: cob was missing");

    let Updated { object, .. } = update(
        &storage,
        &signer,
        proj.project.content_id,
        vec![],
        signer.public_key(),
        Update {
            changes: nonempty!(b"issue 1".to_vec()),
            object_id: *cob.id(),
            type_name: typename.clone(),
            embeds: vec![],
            message: "commenting xyz.rad.issue".to_string(),
        },
    )
    .unwrap();

    let expected = get(&storage, &typename, object.id())
        .unwrap()
        .expect("BUG: cob was missing");

    assert_ne!(object, not_expected);
    assert_eq!(object, expected, "{object:#?} {expected:#?}");
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
        person: terry,
    };
    let neil_proj = test::RemoteProject {
        project: proj,
        person: neil,
    };
    let typename = "xyz.rad.issue".parse::<TypeName>().unwrap();
    let cob = create::<NonEmpty<Entry>, _, _>(
        &storage,
        &terry_signer,
        terry_proj.project.content_id,
        vec![],
        terry_signer.public_key(),
        Create {
            contents: nonempty!(b"issue 1".to_vec()),
            type_name: typename.clone(),
            message: "creating xyz.rad.issue".to_string(),
            embeds: vec![],
            version: Version::default(),
        },
    )
    .unwrap();
    copy_to(
        storage.as_raw(),
        terry_signer.public_key(),
        &neil_proj,
        &typename,
        *cob.id(),
    )
    .unwrap();

    let Updated { object, .. } = update::<NonEmpty<Entry>, _, _>(
        &storage,
        &neil_signer,
        neil_proj.project.content_id,
        vec![],
        neil_signer.public_key(),
        Update {
            changes: nonempty!(b"issue 2".to_vec()),
            object_id: *cob.id(),
            type_name: typename,
            embeds: vec![],
            message: "commenting on xyz.rad.issue".to_string(),
        },
    )
    .unwrap();

    let root = object.history.root().id;
    // traverse over the history and filter by changes that were only authorized by terry
    let contents = object
        .history()
        .traverse(Vec::new(), &[root], |mut acc, _, entry| {
            if entry.author() == terry_signer.public_key() {
                acc.push(entry.contents().head.clone());
            }
            ControlFlow::Continue(acc)
        });

    assert_eq!(contents, vec![b"issue 1".to_vec()]);

    // traverse over the history and filter by changes that were only authorized by neil
    let contents = object
        .history()
        .traverse(Vec::new(), &[root], |mut acc, _, entry| {
            acc.push(entry.contents().head.clone());
            ControlFlow::Continue(acc)
        });

    assert_eq!(contents, vec![b"issue 1".to_vec(), b"issue 2".to_vec()]);
}

#[quickcheck]
fn parse_refstr(oid: ObjectId, typename: TypeName) {
    let suffix = refname!("refs/cobs")
        .and(Component::from(&typename))
        .and(Component::from(&oid));

    // refs/cobs/<typename>/<object_id> gives back the <typename> and <object_id>
    assert_eq!(object::parse_refstr(&suffix), Some((typename.clone(), oid)));

    // strips a single namespace
    assert_eq!(
        object::parse_refstr(&refname!("refs/namespaces/a").join(&suffix)),
        Some((typename.clone(), oid))
    );

    // strips multiple namespaces
    assert_eq!(
        object::parse_refstr(&refname!("refs/namespaces/a/refs/namespaces/b").join(&suffix)),
        Some((typename.clone(), oid))
    );

    // ignores the extra path
    assert_eq!(
        object::parse_refstr(
            &refname!("refs/namespaces/a/refs/namespaces/b")
                .join(suffix)
                .and(refname!("more/paths"))
        ),
        Some((typename, oid))
    );
}

/// Note: an invalid type name is also an invalid reference string, it
/// cannot start or end with a '.', and cannot have '..'.
#[quickcheck]
fn invalid_parse_refstr(oid: Invalid<ObjectId>, typename: TypeName) {
    let oid = RefString::try_from(oid.value).unwrap();
    let typename = Component::from(&typename);
    let suffix = refname!("refs/cobs").and(typename).and(oid);

    // All parsing will fail because `oid` is not a valid ObjectId
    assert_eq!(object::parse_refstr(&suffix), None);

    assert_eq!(
        object::parse_refstr(&refname!("refs/namespaces/a").join(&suffix)),
        None
    );

    assert_eq!(
        object::parse_refstr(&refname!("refs/namespaces/a/refs/namespaces/b").join(&suffix)),
        None
    );

    assert_eq!(
        object::parse_refstr(
            &refname!("refs/namespaces/a/refs/namespaces/b")
                .join(suffix)
                .and(refname!("more/paths"))
        ),
        None
    );
}

fn gen<T: Arbitrary>(size: usize) -> T {
    let mut gen = qcheck::Gen::new(size);

    T::arbitrary(&mut gen)
}

fn copy_to(
    repo: &git2::Repository,
    from: &PublicKey,
    to: &test::RemoteProject,
    typename: &TypeName,
    object: ObjectId,
) -> Result<(), git2::Error> {
    let original = {
        let name = format!("refs/rad/{}/cobs/{}/{}", from, typename, object);
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
