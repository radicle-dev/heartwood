use std::path::Path;

use crypto::test::signer::MockSigner;
use crypto::{PublicKey, Signer as _, signature};
use qcheck::TestResult;
use qcheck_macros::quickcheck;
use radicle_core::{NodeId, RepoId};
use radicle_git_metadata::author::{Author, Time};
use radicle_oid::Oid;
use tempfile::TempDir;

use crate::git;
use crate::storage::refs::sigrefs::VerifiedCommit;
use crate::storage::refs::sigrefs::read::{SignedRefsReader, Tip};
use crate::storage::refs::sigrefs::write::{SignedRefsWriter, Update};
use crate::storage::refs::{IDENTITY_ROOT, Refs};

use super::Committer;

/// Newtype wrapper around [`Vec`] to keep the [`Arbitrary`] implementation
/// bounded to a smaller size.
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

/// A helper structure that sets up a minimal Radicle repository.
struct Repository {
    raw: git::raw::Repository,
    rid: RepoId,
    root: Oid,
    _tmp: TempDir,
}

impl Repository {
    fn new() -> Repository {
        let dir = TempDir::new().unwrap();
        let repo = git::raw::Repository::init_bare(dir.path()).unwrap();
        let (root, rid) = Self::write_identity(&repo);
        Repository {
            raw: repo,
            rid,
            root,
            _tmp: dir,
        }
    }

    /// Writes a mock blob that represents the identity document, and creates a
    /// commit in the repository that contains this blob.
    fn write_identity(repo: &git::raw::Repository) -> (Oid, RepoId) {
        let blob = repo.blob(b"identity root").unwrap();
        let empty = {
            let empty = repo.treebuilder(None).unwrap();
            let tree = empty.write().unwrap();
            repo.find_tree(tree).unwrap()
        };
        let tree = {
            let mut tb = git::raw::build::TreeUpdateBuilder::new();
            tb.upsert(
                Path::new("embeds").join(*crate::identity::doc::PATH),
                blob,
                git::raw::FileMode::Blob,
            );
            let tree = tb.create_updated(repo, &empty).unwrap();
            repo.find_tree(tree).unwrap()
        };
        let author = git::raw::Signature::now("testy", "testy@example.com").unwrap();
        let root = repo
            .commit(
                Some(IDENTITY_ROOT.as_str()),
                &author,
                &author,
                "identity root",
                &tree,
                &[],
            )
            .unwrap();
        (root.into(), RepoId::from(blob))
    }
}

fn write_log(
    refs: Refs,
    rid: RepoId,
    namespace: NodeId,
    signer: &MockSigner,
    repo: &git::raw::Repository,
) -> Update {
    SignedRefsWriter::new(refs, rid, namespace, repo, signer)
        .write(
            mock_committer(),
            "test commit".to_string(),
            "test reflog".to_string(),
        )
        .unwrap()
}

fn read_log(
    rid: RepoId,
    namespace: NodeId,
    verifier: &Verifier,
    repo: &git::raw::Repository,
) -> VerifiedCommit {
    SignedRefsReader::new(rid, Tip::Reference(namespace), repo, verifier)
        .read()
        .unwrap()
}

#[quickcheck]
fn initial_commit_roundtrip(mut refs: Refs) -> bool {
    let Repository {
        raw: repo,
        rid,
        root,
        _tmp,
    } = Repository::new();
    refs.insert(IDENTITY_ROOT.to_ref_string(), root);
    let signer = MockSigner::default();
    let namespace = *signer.public_key();
    let verifier = Verifier::new(&signer);

    let update = write_log(refs.clone(), rid, namespace, &signer, &repo);
    let head_oid = match update {
        Update::Changed { ref entry, .. } => entry.oid(),
        Update::Unchanged { .. } => return false,
    };

    let verified_commit = read_log(rid, namespace, &verifier, &repo);
    let head = *verified_commit.commit().oid();
    let parent = verified_commit.commit().parent().copied();
    let new_refs = verified_commit.into_sigrefs_at(namespace);

    head == head_oid && parent.is_none() && *new_refs == refs
}

#[quickcheck]
fn chain_roundtrip(chain: BoundedVec<Refs>) -> TestResult {
    let chain = chain.0;
    if chain.is_empty() {
        return TestResult::discard();
    }

    let Repository {
        raw: repo,
        rid,
        root,
        _tmp,
    } = Repository::new();
    let signer = MockSigner::default();
    let namespace = *signer.public_key();
    let verifier = Verifier::new(&signer);

    let mut last_changed_head = None;
    let mut expected_parent = None;

    for mut refs in chain {
        refs.insert(IDENTITY_ROOT.to_ref_string(), root);
        let update = write_log(refs.clone(), rid, namespace, &signer, &repo);

        if let Update::Changed { ref entry, .. } = update {
            last_changed_head = Some(entry.oid());
        }

        let verified_commit = read_log(rid, namespace, &verifier, &repo);
        let head = *verified_commit.commit().oid();
        let parent = verified_commit.commit().parent().copied();
        let new_refs = verified_commit.into_sigrefs_at(namespace);

        if refs != *new_refs {
            return TestResult::failed();
        }

        if let Some(expected_head) = last_changed_head {
            if head != expected_head {
                return TestResult::error(format!(
                    "expected commit to be {expected_head}, but found {head}"
                ));
            }
            if parent != expected_parent {
                return TestResult::error(format!(
                    "expected parent commit to be {expected_parent:?}, but found {parent:?}"
                ));
            }
        }
        expected_parent = Some(head);
    }

    TestResult::passed()
}

#[quickcheck]
fn idempotent_write(mut refs: Refs) -> bool {
    let Repository {
        raw: repo,
        rid,
        root,
        _tmp,
    } = Repository::new();
    refs.insert(IDENTITY_ROOT.to_ref_string(), root);
    let signer = MockSigner::default();
    let namespace = *signer.public_key();

    let first = write_log(refs.clone(), rid, namespace, &signer, &repo);
    let head_oid = match first {
        Update::Changed { ref entry, .. } => entry.oid(),
        Update::Unchanged { .. } => return false,
    };

    let second = write_log(refs, rid, namespace, &signer, &repo);
    matches!(second, Update::Unchanged { verified } if *verified.commit().oid() == head_oid)
}
