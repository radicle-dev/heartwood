use radicle_oid::Oid;

use crate::storage::refs::sigrefs::git::object;

use super::{Commit, CommitReader, error};

pub(super) struct Walk<'a, R> {
    repository: &'a R,
    cursor: Option<Oid>,
}

impl<'a, R> Walk<'a, R> {
    pub fn new(head: Oid, repository: &'a R) -> Self {
        Self {
            repository,
            cursor: Some(head),
        }
    }
}

impl<'a, R: object::Reader> Iterator for Walk<'a, R> {
    type Item = Result<Commit, error::Commit>;

    fn next(&mut self) -> Option<Self::Item> {
        match self
            .cursor
            .map(|commit| CommitReader::new(commit, self.repository).read())
        {
            None => None,
            Some(Ok(commit)) => {
                self.cursor = commit.parent;
                Some(Ok(commit))
            }
            Some(Err(err)) => Some(Err(err)),
        }
    }
}
