#![allow(clippy::unwrap_used)]

use crypto::{signature, test::signer::MockSigner, PublicKey, Signer as _};
use qcheck::TestResult;
use qcheck_macros::quickcheck;
use radicle_core::{NodeId, RepoId};
use radicle_git_metadata::author::{Author, Time};
use radicle_oid::Oid;
use tempfile::TempDir;

use crate::storage::refs::sigrefs::{
    read::{CheckpointReason, Latest, SignedRefsReader, Tip},
    write::{Committer, SignedRefsWriter, Update},
};
use crate::storage::refs::Refs;

#[derive(Clone, Debug)]
struct BoundedVec<T>(Vec<T>);

impl<T: qcheck::Arbitrary> qcheck::Arbitrary for BoundedVec<T> {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let size = usize::arbitrary(g) % 24;
        BoundedVec((0..size).map(|_| T::arbitrary(g)).collect())
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(self.0.shrink().map(BoundedVec))
    }
}

struct Verifier {
    key: PublicKey,
}

impl Verifier {
    fn new(signer: &MockSigner) -> Self {
        Self {
            key: *signer.public_key(),
        }
    }
}

impl signature::Verifier<crypto::Signature> for Verifier {
    fn verify(&self, msg: &[u8], signature: &crypto::Signature) -> Result<(), signature::Error> {
        self.key
            .verify(msg, signature)
            .map_err(signature::Error::from_source)
    }
}

fn mock_author() -> Author {
    Author {
        name: "testy".to_string(),
        email: "testy@example.com".to_string(),
        time: Time::new(6400, 0),
    }
}

fn mock_committer() -> Committer {
    Committer::new(mock_author())
}

fn setup() -> (TempDir, git2::Repository) {
    let dir = TempDir::new().unwrap();
    let repo = git2::Repository::init_bare(dir.path()).unwrap();
    (dir, repo)
}

fn mock_root() -> RepoId {
    RepoId::from(Oid::from_sha1([1; 20]))
}

fn write_log(
    refs: Refs,
    namespace: NodeId,
    signer: &MockSigner,
    repo: &git2::Repository,
) -> Update {
    SignedRefsWriter::new(namespace, signer, repo)
        .with_refs(refs)
        .write(
            mock_committer(),
            "test commit".to_string(),
            "test reflog".to_string(),
        )
        .unwrap()
}

fn read_log(namespace: NodeId, verifier: &Verifier, repo: &git2::Repository) -> Latest {
    SignedRefsReader::new(mock_root(), Tip::Reference(namespace), repo, verifier)
        .read()
        .unwrap()
        .unwrap()
}

#[quickcheck]
fn initial_commit_roundtrip(refs: Refs) -> bool {
    let (_dir, repo) = setup();
    let signer = MockSigner::default();
    let namespace = *signer.public_key();
    let verifier = Verifier::new(&signer);

    let update = write_log(refs.clone(), namespace, &signer, &repo);
    let head_oid = match update {
        Update::Changed { ref entry } => *entry.oid(),
        Update::Unchanged { .. } => return false,
    };

    let Latest {
        refs: expected,
        checkpoint,
        ..
    } = read_log(namespace, &verifier, &repo);

    checkpoint.head() == head_oid
        && checkpoint.ancestor() == head_oid
        && checkpoint.reason() == CheckpointReason::Root
        && expected == refs
}

#[quickcheck]
fn chain_roundtrip(chain: BoundedVec<Refs>) -> TestResult {
    let chain = chain.0;
    if chain.is_empty() {
        return TestResult::discard();
    }

    let (_dir, repo) = setup();
    let signer = MockSigner::default();
    let namespace = *signer.public_key();
    let verifier = Verifier::new(&signer);

    let mut last_changed_head = None;

    for refs in chain {
        let update = write_log(refs.clone(), namespace, &signer, &repo);

        if let Update::Changed { ref entry } = update {
            last_changed_head = Some(*entry.oid());
        }

        let Latest {
            refs: expected,
            checkpoint,
            ..
        } = read_log(namespace, &verifier, &repo);

        if refs != expected {
            return TestResult::failed();
        }

        if checkpoint.reason() != CheckpointReason::Root {
            return TestResult::failed();
        }

        if let Some(expected_head) = last_changed_head {
            if checkpoint.head() != expected_head {
                return TestResult::failed();
            }
        }
    }

    TestResult::passed()
}

#[quickcheck]
fn idempotent_write(refs: Refs) -> bool {
    let (_dir, repo) = setup();
    let signer = MockSigner::default();
    let namespace = *signer.public_key();

    let first = write_log(refs.clone(), namespace, &signer, &repo);
    let head_oid = match first {
        Update::Changed { ref entry } => *entry.oid(),
        Update::Unchanged { .. } => return false,
    };

    let second = write_log(refs, namespace, &signer, &repo);
    matches!(second, Update::Unchanged { commit, .. } if commit == head_oid)
}
