pub mod error;

mod iter;
pub use iter::OpsIter;
use iter::Walk;

use std::marker::PhantomData;

use serde::Deserialize;

use crate::git;
use crate::git::Oid;

use super::{ObjectId, Op, TypeName};

/// Helper trait for anything can provide its initial commit. Generally, this is
/// the root of a COB object.
pub trait HasRoot {
    /// Return the root `Oid` of the COB.
    fn root(&self) -> Oid;
}

/// Provide the stream of operations that are related to a given COB.
///
/// The whole history of operations can be retrieved via [`CobStream::all`].
///
/// To constrain the history, use one of [`CobStream::since`],
/// [`CobStream::until`], or [`CobStream::range`].
pub trait CobStream: HasRoot {
    /// Any error that can occur when iterating over the operations.
    type IterError: std::error::Error + Send + Sync + 'static;

    /// The associated action to the COB's [`Op`].
    type Action;

    /// The iterator that walks over the operations.
    type Iter: Iterator<Item = Result<Op<Self::Action>, Self::IterError>>;

    /// Get an iterator of all operations from the inception of the collaborative
    /// object.
    fn all(&self) -> Result<Self::Iter, error::Stream>;

    /// Get an iterator of all operations from the given `oid`, in the
    /// collaborative object's history.
    fn since(&self, oid: Oid) -> Result<Self::Iter, error::Stream>;

    /// Get an iterator of all operations until the given `oid`, in the
    /// collaborative object's history.
    fn until(&self, oid: Oid) -> Result<Self::Iter, error::Stream>;

    /// Get an iterator of all operations `from` the given `Oid`, `until` the
    /// other `Oid`, in the collaborative object's history.
    fn range(&self, from: Oid, until: Oid) -> Result<Self::Iter, error::Stream>;
}

/// The range for iterating over a COB's action history.
///
/// Construct via [`CobRange::new`] to use for constructing a [`Stream`].
#[derive(Clone, Debug)]
pub struct CobRange {
    root: Oid,
    until: iter::Until,
}

impl CobRange {
    /// Construct a `CobRange` for a given COB [`TypeName`] and its
    /// [`ObjectId`] identifier.
    ///
    /// The range will be from the root, given by the [`ObjectId`], to the
    /// reference tips of all remote namespaces.
    pub fn new(typename: &TypeName, object_id: &ObjectId) -> Self {
        let glob = crate::storage::refs::cobs(typename, object_id);
        Self {
            root: **object_id,
            until: iter::Until::Glob(glob),
        }
    }
}

impl HasRoot for CobRange {
    fn root(&self) -> Oid {
        self.root
    }
}

/// A stream over a COB's operations.
///
/// The generic parameter `A` is filled by the COB's corresponding `Action`
/// type.
///
/// The `Stream` implements [`CobStream`], so iterators over the operations can be
/// constructed via the [`CobStream`] methods.
///
/// To construct a `Stream`, use [`Stream::new`].
pub struct Stream<'a, A> {
    repo: &'a git::raw::Repository,
    range: CobRange,
    typename: TypeName,
    marker: PhantomData<A>,
}

impl<'a, A> Stream<'a, A> {
    /// Construct a new stream providing the underlying `repo`, a [`CobRange`],
    /// and the [`TypeName`] of the COB that is being streamed.
    pub fn new(repo: &'a git::raw::Repository, range: CobRange, typename: TypeName) -> Self {
        Self {
            repo,
            range,
            typename,
            marker: PhantomData,
        }
    }
}

impl<A> HasRoot for Stream<'_, A> {
    fn root(&self) -> Oid {
        self.range.root()
    }
}

impl<'a, A> CobStream for Stream<'a, A>
where
    A: for<'de> Deserialize<'de>,
{
    type IterError = error::Ops;
    type Action = A;
    type Iter = OpsIter<'a, Self::Action>;

    fn all(&self) -> Result<Self::Iter, error::Stream> {
        Ok(OpsIter::new(
            Walk::from(self.range.clone())
                .iter(self.repo)
                .map_err(error::Stream::new)?,
            self.typename.clone(),
        ))
    }

    fn since(&self, oid: Oid) -> Result<Self::Iter, error::Stream> {
        Ok(OpsIter::new(
            Walk::from(self.range.clone())
                .since(oid)
                .iter(self.repo)
                .map_err(error::Stream::new)?,
            self.typename.clone(),
        ))
    }

    fn until(&self, oid: Oid) -> Result<Self::Iter, error::Stream> {
        Ok(OpsIter::new(
            Walk::from(self.range.clone())
                .until(oid)
                .iter(self.repo)
                .map_err(error::Stream::new)?,
            self.typename.clone(),
        ))
    }

    fn range(&self, from: Oid, until: Oid) -> Result<Self::Iter, error::Stream> {
        Ok(OpsIter::new(
            Walk::new(from, until.into())
                .iter(self.repo)
                .map_err(error::Stream::new)?,
            self.typename.clone(),
        ))
    }
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, fmt};

    use json::json;
    use nonempty::NonEmpty;
    use serde_json as json;

    use crate::cob::change::Storage as _;
    use crate::crypto::test::signer::MockSigner;
    use crate::test::arbitrary;
    use crate::test::arbitrary::r#gen;
    use crate::{cob, test};

    use super::*;

    fn typename() -> TypeName {
        "xyz.radicle.test".parse::<TypeName>().unwrap()
    }

    fn gen_ops(repo: &git::raw::Repository, signer: &MockSigner) -> Vec<cob::Entry> {
        // Number of ops
        let n = r#gen::<u8>(1).clamp(1, 10);
        let mut entries = Vec::with_capacity(n.into());

        let mut parent = None;
        for _ in 0..n {
            // Number of actions in this op
            let m = r#gen::<u8>(1).clamp(1, 3);
            let contents = create_contents((0..m).map(|_| arbitrary::alphanumeric(1)));
            let entry = create_entry(repo, signer, contents, parent);
            parent = Some(entry.id);
            entries.push(entry);
        }
        entries
    }

    fn create_contents(values: impl Iterator<Item = String>) -> NonEmpty<Vec<u8>> {
        NonEmpty::collect(values.map(|value| {
            json::to_vec(&json!({
                "test": value,
            }))
            .unwrap()
        }))
        .unwrap()
    }

    fn create_entry(
        repo: &git::raw::Repository,
        signer: &MockSigner,
        contents: NonEmpty<Vec<u8>>,
        parent: Option<Oid>,
    ) -> cob::Entry {
        repo.store(
            None,
            parent.into_iter().collect(),
            signer,
            cob::change::Template {
                type_name: typename(),
                tips: vec![],
                message: "Test Op Stream".to_string(),
                embeds: vec![],
                contents,
            },
        )
        .unwrap()
    }

    /// all === from(root)
    fn prop_all_from<S>(stream: &S)
    where
        S: CobStream,
        S::Action: fmt::Debug + Eq,
    {
        assert_eq!(
            stream
                .all()
                .expect("failed to get 'all' stream")
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            stream
                .since(stream.root())
                .expect("failed to get 'from' stream")
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        )
    }

    /// all === until(tip)
    fn prop_all_until<S>(stream: &S, tip: Oid)
    where
        S: CobStream,
        S::Action: fmt::Debug + Eq,
    {
        assert_eq!(
            stream
                .all()
                .expect("failed to get 'all' stream")
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            stream
                .until(tip)
                .expect("failed to get 'until' stream")
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        )
    }

    /// all === from_until(root, tip)
    fn prop_all_from_until<S>(stream: &S, tip: Oid)
    where
        S: CobStream,
        S::Action: fmt::Debug + Eq,
    {
        let root = stream.root();
        assert_eq!(
            stream
                .all()
                .expect("failed to get 'all' stream")
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            stream
                .range(root, tip)
                .expect("failed to get 'from_until' stream")
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            "from: {root}, until: {tip}"
        )
    }

    /// from_until(a, b) === from(a).intersect(until(b))
    fn prop_from_until<S>(stream: &S, from: Oid, until: Oid)
    where
        S: CobStream,
        S::Action: fmt::Debug + Clone,
    {
        let from_s = stream
            .since(from)
            .expect("failed to get 'from' stream")
            .map(|op| op.expect("Op failed in stream").id)
            .collect::<BTreeSet<_>>();

        let until_s = stream
            .until(until)
            .expect("failed to get 'until' stream")
            .map(|op| op.expect("Op failed in stream").id)
            .collect::<BTreeSet<_>>();
        let from_until_s = stream
            .range(from, until)
            .expect("failed to get 'from_until' stream")
            .map(|op| op.unwrap().id)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            from_s
                .intersection(&until_s)
                .cloned()
                .collect::<BTreeSet<_>>(),
            from_until_s,
            "\nfrom_until: {from_until_s:?}\nfrom: {from_s:?}\nuntil: {until_s:?}"
        )
    }

    #[test]
    fn test_all_from() {
        let tmp = tempfile::tempdir().unwrap();
        let (repo, _) = test::fixtures::repository(tmp.path());
        let signer = MockSigner::default();
        let ops = gen_ops(&repo, &signer);
        let history = CobRange {
            root: ops.first().unwrap().id,
            until: ops.last().unwrap().id.into(),
        };
        let stream = Stream::<json::Value>::new(&repo, history, typename());
        prop_all_from(&stream)
    }

    #[test]
    fn test_all_until() {
        let tmp = tempfile::tempdir().unwrap();
        let (repo, _) = test::fixtures::repository(tmp.path());
        let signer = MockSigner::default();
        let ops = gen_ops(&repo, &signer);
        let tip = ops.last().unwrap().id;
        let history = CobRange {
            root: ops.first().unwrap().id,
            until: tip.into(),
        };
        let stream = Stream::<json::Value>::new(&repo, history, typename());
        prop_all_until(&stream, tip)
    }

    #[test]
    fn test_all_from_until() {
        let tmp = tempfile::tempdir().unwrap();
        let (repo, _) = test::fixtures::repository(tmp.path());
        let signer = MockSigner::default();
        let ops = gen_ops(&repo, &signer);
        let tip = ops.last().unwrap().id;
        let history = CobRange {
            root: ops.first().unwrap().id,
            until: tip.into(),
        };
        let stream = Stream::<json::Value>::new(&repo, history, typename());
        prop_all_from_until(&stream, tip)
    }

    #[test]
    fn test_from_until() {
        let tmp = tempfile::tempdir().unwrap();
        let (repo, _) = test::fixtures::repository(tmp.path());
        let signer = MockSigner::default();
        let ops = gen_ops(&repo, &signer);
        let history = CobRange {
            root: ops.first().unwrap().id,
            until: ops.last().unwrap().id.into(),
        };
        let n = ops.len() - 1;
        let (x, y) = r#gen::<(usize, usize)>(1);
        let x = x.clamp(0, n);
        let y = y.clamp(0, n);
        let (from, until) = if x <= y {
            (ops[x].id, ops[y].id)
        } else {
            (ops[y].id, ops[x].id)
        };
        let stream = Stream::<json::Value>::new(&repo, history, typename());
        prop_from_until(&stream, from, until)
    }

    #[test]
    fn test_regression_from_until() {
        let tmp = tempfile::tempdir().unwrap();
        let (repo, _) = test::fixtures::repository(tmp.path());
        let signer = MockSigner::default();
        // Set up 3 entries that make up the COB history
        let op1 = create_entry(
            &repo,
            &signer,
            create_contents(std::iter::once("hello".to_string())),
            None,
        );
        let op2 = create_entry(
            &repo,
            &signer,
            create_contents(std::iter::once("radicle".to_string())),
            Some(op1.id),
        );
        let op3 = create_entry(
            &repo,
            &signer,
            create_contents(std::iter::once("world".to_string())),
            Some(op2.id),
        );

        // The history spans from the 1st op to the last
        let history = CobRange {
            root: op1.id,
            until: op3.id.into(),
        };
        let stream = Stream::<json::Value>::new(&repo, history, typename());
        eprintln!("Op 1: {}", op1.id);
        eprintln!("Op 2: {}", op2.id);
        eprintln!("Op 3: {}", op3.id);

        // "since" the root operation should include all operations
        assert_eq!(
            stream
                .since(op1.id)
                .unwrap()
                .map(|op| op.unwrap().id)
                .collect::<BTreeSet<_>>(),
            [op1.id, op2.id, op3.id]
                .into_iter()
                .collect::<BTreeSet<_>>()
        );
        // "until" the second operation should only include up to the second operation
        assert_eq!(
            stream
                .until(op2.id)
                .unwrap()
                .map(|op| op.unwrap().id)
                .collect::<BTreeSet<_>>(),
            [op1.id, op2.id].into_iter().collect::<BTreeSet<_>>()
        );
        prop_from_until(&stream, op1.id, op2.id);
    }
}
