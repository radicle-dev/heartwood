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
        profile::Paths,
        test::{fixtures, storage::git::Repository},
        Storage,
    };

    pub fn context(tmp: &TempDir) -> (Storage, MockSigner, Repository) {
        let mut rng = fastrand::Rng::new();
        let signer = MockSigner::new(&mut rng);
        let home = tmp.path().join("home");
        let paths = Paths::new(home.as_path());
        let storage = Storage::open(paths.storage()).unwrap();
        let (id, _, _, _) = fixtures::project(tmp.path().join("copy"), &storage, &signer).unwrap();
        let project = storage.repository(id).unwrap();

        (storage, signer, project)
    }
}
