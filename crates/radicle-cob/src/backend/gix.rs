pub mod change;

pub type Oid = gix::ObjectId;

/// Environment variable to set to overwrite the commit date for both the author and the committer.
///
/// The format must be a unix timestamp.
#[deprecated]
pub(crate) const GIT_COMMITTER_DATE: &str = "GIT_COMMITTER_DATE";
