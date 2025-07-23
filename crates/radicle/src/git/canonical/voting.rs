use std::collections::BTreeMap;

use crate::git::Oid;

use super::MergeBase;

/// Keep track of [`Votes`] for quorums involving tag objects.
#[derive(Debug, Default)]
pub struct TagVoting {
    votes: Votes,
}

impl TagVoting {
    /// Build the initial set of votes given the set of `targets`. Each [`Oid`]
    /// will provide a single vote, where repeated [`Oid`]s will increment the
    /// vote count.
    pub fn from_targets(targets: impl Iterator<Item = Oid>) -> Self {
        let votes = targets.fold(Votes::default(), |mut votes, oid| {
            votes.vote(oid);
            votes
        });
        Self { votes }
    }

    /// Finish the voting process and get the [`Votes`] from the
    /// [`TagVoting`].
    pub fn votes(self) -> Votes {
        self.votes
    }
}

/// Keep track of [`Votes`] for quorums involving commit objects.
///
/// Build a list of candidate commits and count how many "votes" each of them
/// has. Commits get a point for each direct vote, as well as for being part of
/// the ancestry of a commit given to this function.
#[derive(Debug, Default)]
pub struct CommitVoting {
    candidates: Vec<(Oid, Vec<Oid>)>,
    votes: Votes,
}

impl CommitVoting {
    /// Build the initial set of votes given the set of `targets`. Each [`Oid`]
    /// will provide a single vote, where repeated [`Oid`]s will increment the
    /// vote count.
    ///
    /// It will also build the candidates which can be produced using the
    /// [`CommitVoting::next_candidate`] method.
    pub fn from_targets(targets: impl Iterator<Item = Oid> + Clone) -> Self {
        let ts = targets.clone();
        let (candidates, votes) = targets.enumerate().fold(
            (Vec::new(), Votes::default()),
            |(mut candidates, mut votes), (i, oid)| {
                candidates.push((oid, ts.clone().skip(i + 1).collect()));
                votes.vote(oid);
                (candidates, votes)
            },
        );
        Self { candidates, votes }
    }

    /// Get the next candidate to be considered for ancestry votes.
    ///
    /// The first of each pair will be the candidate commit, which should be
    /// compared to the other commit to see what their common merge base is. The
    /// merge base is then recorded using [`MergeBase`] and is recorded using
    /// [`CommitVoting::found_merge_base`].
    pub fn next_candidate(&mut self) -> Option<impl Iterator<Item = (Oid, Oid)>> {
        self.candidates
            .pop()
            .map(|(oid, others)| others.into_iter().map(move |other| (oid, other)))
    }

    /// Record a merge base, and add to the vote if it counts towards the
    /// result.
    pub fn found_merge_base(&mut self, merge_base: MergeBase) {
        // Avoid double counting the same commits
        if let Some(oid) = merge_base.linear() {
            self.votes.vote(oid)
        }
    }

    /// Finish the voting process and get the [`Votes`] from the
    /// [`CommitVoting`].
    pub fn votes(self) -> Votes {
        self.votes
    }
}

/// Count the number of votes per [`Oid`].
///
/// Note that the count cannot exceed 255, since that is the maximum number the
/// `threshold` value can be.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Votes {
    inner: BTreeMap<Oid, u8>,
}

impl Votes {
    /// Increase the vote count for `oid`.
    ///
    /// If `oid` does not exist in the set of [`Votes`] yet, then no vote will
    /// be added.
    #[inline]
    fn vote(&mut self, oid: Oid) {
        let votes = self.inner.entry(oid).or_default();
        *votes = votes.saturating_add(1);
    }

    /// Filter the candidates by the ones that have a number of votes that pass
    /// the `threshold`.
    #[inline]
    pub fn candidates_past_threshold(&mut self, threshold: usize) {
        self.inner.retain(|_, votes| *votes as usize >= threshold);
    }

    /// Get the number of candidates this set of votes has.
    #[inline]
    pub fn number_of_candidates(&self) -> usize {
        self.inner.len()
    }

    /// Get the set candidates.
    #[inline]
    pub fn candidates(&self) -> impl Iterator<Item = &Oid> {
        self.inner.keys()
    }

    /// Pop off the first candidate from the set of votes.
    #[inline]
    pub fn pop_first_candidate(&mut self) -> Option<Oid> {
        self.inner.pop_first().map(|(oid, _)| oid)
    }

    /// Get the candidate with the most votes.
    #[inline]
    pub fn max_candidate(&self) -> Option<&Oid> {
        self.inner
            .iter()
            .max_by(|(_, x), (_, y)| x.cmp(y))
            .map(|(oid, _)| oid)
    }
}
