pub(crate) mod io {
    use radicle::git::canonical::error::QuorumError;

    use crate::push::error;
    use crate::warn;

    /// Handle recoverable errors, printing relevant information to the
    /// terminal. Otherwise, convert the error into an unrecoverable error
    /// [`error::CanonicalUnrecoverable`].
    pub(crate) fn handle_error(e: QuorumError) -> Result<(), error::CanonicalUnrecoverable> {
        match e {
            QuorumError::Convergence(err) => Err(err.into()),
            QuorumError::MergeBase(err) => Err(err.into()),
            e @ QuorumError::DivergingCommits { .. } => {
                warn(e.to_string());
                warn("it is recommended to find a commit to agree upon");
                Ok(())
            }
            e @ QuorumError::DivergingTags { .. } => {
                warn(e.to_string());
                warn("it is recommended to find a tag to agree upon");
                Ok(())
            }
            e @ QuorumError::DifferentTypes { .. } => {
                warn(e.to_string());
                warn(
                    "it is recommended to find an object type (either commit or tag) to agree upon",
                );
                Ok(())
            }
            e @ QuorumError::NoCandidates { .. } => {
                warn(e.to_string());
                warn("it is recommended to find an object (either commit or tag) to agree upon");
                Ok(())
            }
        }
    }
}
