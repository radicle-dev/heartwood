use std::{fmt, ops::ControlFlow};

use crate::git::Oid;
use crate::prelude::Did;

use super::{effects, error, Object};

/// Checks for convergence and ensures that compared objects are of the same
/// type, i.e. commit or tag, to the [`Candidate`].
pub(super) struct Convergence<'r, R> {
    repo: &'r R,
    checker: Candidate,
}

impl<'r, R> fmt::Debug for Convergence<'r, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Convergence")
            .field("checker", &self.checker)
            .finish()
    }
}

impl<'r, R> Convergence<'r, R>
where
    R: effects::Ancestry,
{
    pub fn new(repo: &'r R, candidate: Did, object: Object) -> Self {
        Self {
            repo,
            checker: Candidate::new(candidate, object),
        }
    }

    /// For each voter in `voters`:
    ///   1. If the [`Object`] is a commit:
    ///
    ///    a. Ensure that the candidate and voting commit have a linear
    ///       relationship.
    ///    b. That [`Object`]'s type matches type of the [`Candidate`].
    ///
    ///   2. If the [`Object`] is a tag, then ensure the [`Candidate`] object is
    ///      a tag.
    ///   3. Always skip a vote that is the same as the [`Candidate`].
    pub fn check<'a, I>(self, voters: I) -> Result<Option<(Did, Object)>, error::ConvergesError>
    where
        I: Iterator<Item = (&'a Did, &'a Object)>,
    {
        let mut converges = false;
        for (did, object) in voters {
            match self.checker.compare_to_candidate(did, *object) {
                ControlFlow::Continue(c) => match c {
                    Effect::GraphCheck { commit, upstream } => {
                        converges |= self.repo.graph_ahead_behind(commit, upstream)?.is_linear();
                    }
                    Effect::TagConverges => {
                        converges = true;
                        continue;
                    }
                    Effect::SkipSelf => continue,
                },
                ControlFlow::Break(ConvergenceMismatch { expected, found }) => {
                    return Err(error::ConvergesError::mismatched_object(
                        expected.id(),
                        found.object_type(),
                        expected.object_type(),
                    ));
                }
            }
        }
        Ok(converges.then_some((self.checker.candidate, self.checker.object)))
    }
}

/// The candidate and their [`Object`] they are attempting to converge with.
#[derive(Debug)]
struct Candidate {
    candidate: Did,
    object: Object,
}

/// The "effect" that needs to be performed due to the result of
/// [`Candidate::compare_to_candidate`].
enum Effect {
    /// Perform a check of the commit graph using the `commit` and `upstream`.
    GraphCheck { commit: Oid, upstream: Oid },
    /// Mark that tags always converge – there is no ancestry check.
    TagConverges,
    /// Skip the [`Did`] since it is the same as the [`Candidate`].
    SkipSelf,
}

/// The two [`Object`]s have different types.
pub(super) struct ConvergenceMismatch {
    expected: Object,
    found: Object,
}

impl Candidate {
    fn new(candidate: Did, object: Object) -> Self {
        Self { candidate, object }
    }

    fn compare_to_candidate(
        &self,
        did: &Did,
        object: Object,
    ) -> ControlFlow<ConvergenceMismatch, Effect> {
        if &self.candidate == did {
            return ControlFlow::Continue(Effect::SkipSelf);
        }
        match (self.object, object) {
            (e @ Object::Commit { .. }, f @ Object::Tag { .. })
            | (e @ Object::Tag { .. }, f @ Object::Commit { .. }) => {
                ControlFlow::Break(ConvergenceMismatch {
                    expected: e,
                    found: f,
                })
            }
            (Object::Commit { id: commit }, Object::Commit { id: upstream }) => {
                ControlFlow::Continue(Effect::GraphCheck { commit, upstream })
            }
            (Object::Tag { .. }, Object::Tag { .. }) => ControlFlow::Continue(Effect::TagConverges),
        }
    }
}
