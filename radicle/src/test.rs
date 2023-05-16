#![allow(clippy::unwrap_used)]
pub mod arbitrary;
pub mod assert;
pub mod fixtures;
pub mod storage;

pub mod setup {
    use tempfile::TempDir;

    use crate::crypto::test::signer::MockSigner;
    use crate::prelude::*;
    use crate::{
        git,
        profile::Home,
        rad::REMOTE_NAME,
        test::{fixtures, storage::git::Repository},
        Storage,
    };

    #[derive(Debug)]
    pub struct BranchWith {
        pub base: git::Oid,
        pub oid: git::Oid,
    }

    pub struct Context {
        pub storage: Storage,
        pub signer: MockSigner,
        pub project: Repository,
        pub working: git2::Repository,
    }

    impl Context {
        pub fn new(tmp: &TempDir) -> Self {
            let mut rng = fastrand::Rng::new();
            let signer = MockSigner::new(&mut rng);
            let home = tmp.path().join("home");
            let paths = Home::new(home.as_path()).unwrap();
            let storage = Storage::open(paths.storage()).unwrap();
            let (id, _, working, _) =
                fixtures::project(tmp.path().join("copy"), &storage, &signer).unwrap();
            let project = storage.repository(id).unwrap();

            Self {
                storage,
                signer,
                project,
                working,
            }
        }

        pub fn branch_with(
            &self,
            blobs: impl IntoIterator<Item = (String, Vec<u8>)>,
        ) -> BranchWith {
            let refname = git::Qualified::from(git::lit::refs_heads(git::refname!("master")));
            let base = self.working.refname_to_id(refname.as_str()).unwrap();
            let parent = self.working.find_commit(base).unwrap();
            let oid = commit(&self.working, &refname, blobs, &[&parent]);

            git::push(&self.working, &REMOTE_NAME, [(&refname, &refname)]).unwrap();

            BranchWith {
                base: base.into(),
                oid,
            }
        }
    }

    pub fn initial_blobs() -> Vec<(String, Vec<u8>)> {
        vec![
            ("README.md".to_string(), b"Hello, World!".to_vec()),
            (
                "CONTRIBUTING".to_string(),
                b"Please follow the rules".to_vec(),
            ),
        ]
    }

    pub fn update_blobs() -> Vec<(String, Vec<u8>)> {
        vec![
            ("README.md".to_string(), b"Hello, Radicle!".to_vec()),
            (
                "CONTRIBUTING".to_string(),
                b"Please follow the rules".to_vec(),
            ),
        ]
    }

    pub fn commit(
        repo: &git2::Repository,
        refname: &git::Qualified,
        blobs: impl IntoIterator<Item = (String, Vec<u8>)>,
        parents: &[&git2::Commit<'_>],
    ) -> git::Oid {
        let tree = {
            let mut tb = repo.treebuilder(None).unwrap();
            for (name, blob) in blobs.into_iter() {
                let oid = repo.blob(&blob).unwrap();
                tb.insert(name, oid, git2::FileMode::Blob.into()).unwrap();
            }
            tb.write().unwrap()
        };
        let tree = repo.find_tree(tree).unwrap();
        let author = git2::Signature::now("anonymous", "anonymous@example.com").unwrap();

        repo.commit(
            Some(refname.as_str()),
            &author,
            &author,
            "test commit",
            &tree,
            parents,
        )
        .unwrap()
        .into()
    }
}
