use std::path::Path;
use std::sync::Arc;
use std::{env, fs};

use rand::distributions::{Alphanumeric, DistString};

use radicle::cob::issue::Issues;
use radicle::storage::WriteStorage;
use radicle_cli::commands::rad_init;

use crate::api::Context;

pub const HEAD: &str = "1e978d19f251cd9821d9d9a76d1bd436bf0690d5";
pub const HEAD_1: &str = "f604ce9fd5b7cc77b7609beda45ea8760bee78f7";

pub fn seed() -> Context {
    let random_suffix = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);

    let temp_dir = env::temp_dir()
        .join("radicle-httpd-tests")
        .join(random_suffix);

    let workdir = temp_dir.join("hello-world");
    let rad_home = temp_dir.join("radicle");

    env::set_var("RAD_PASSPHRASE", "asdf");
    env::set_var("RAD_DEBUG", "1");

    // create WORKDIR and RAD_HOME
    fs::create_dir_all(&workdir).unwrap();
    fs::create_dir_all(&rad_home).unwrap();

    // add commits to WORKDIR here
    let repo = radicle::git::raw::Repository::init(&workdir).unwrap();
    let tree =
        radicle::git::write_tree(Path::new("README"), "Hello World!\n".as_bytes(), &repo).unwrap();

    // author and committer signature
    let sig_time = radicle::git::raw::Time::new(1673001014, 0);
    let sig =
        radicle::git::raw::Signature::new("Alice Liddell", "alice@radicle.xyz", &sig_time).unwrap();

    let oid = repo
        .commit(Some("HEAD"), &sig, &sig, "Initial commit\n", &tree, &[])
        .unwrap();
    let commit = repo.find_commit(oid).unwrap();

    repo.checkout_tree(tree.as_object(), None).unwrap();

    fs::create_dir(workdir.join("dir1")).unwrap();
    fs::write(
        workdir.join("dir1").join("README"),
        "Hello World from dir1!\n",
    )
    .unwrap();
    let mut index = repo.index().unwrap();
    index
        .add_all(["."], radicle::git::raw::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();

    let oid = index.write_tree().unwrap();
    let tree = repo.find_tree(oid).unwrap();

    repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        "Add another folder\n",
        &tree,
        &[&commit],
    )
    .unwrap();

    // eq. rad auth
    let profile = radicle::Profile::init(rad_home, "asdf".to_owned()).unwrap();

    // rad init, on repo
    rad_init::init(
        rad_init::Options {
            path: Some(workdir.clone()),
            name: Some("hello-world".to_string()),
            description: Some("Rad repository for tests".to_string()),
            branch: None,
            interactive: false.into(),
            setup_signing: false,
            set_upstream: false,
        },
        &profile,
    )
    .unwrap();

    // eq. rad issue new
    env::set_var("RAD_COMMIT_TIME", "1673001014");

    let signer = radicle_cli::terminal::signer(&profile).unwrap();
    let storage = &profile.storage;
    let (_, id) = radicle::rad::repo(&workdir).unwrap();
    let repo = storage.repository(id).unwrap();
    let mut issues = Issues::open(*signer.public_key(), &repo).unwrap();
    issues
        .create(
            "Issue #1".to_string(),
            "Change 'hello world' to 'hello everyone'".to_string(),
            &[],
            &signer,
        )
        .unwrap();

    Context {
        profile: Arc::new(profile),
        sessions: Default::default(),
    }
}
