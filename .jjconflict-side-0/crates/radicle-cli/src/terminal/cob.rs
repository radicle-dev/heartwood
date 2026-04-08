use radicle::{
    Profile,
    cob::{
        self,
        cache::{MigrateCallback, MigrateProgress},
        store::access::{ReadOnly, WriteAs},
    },
    prelude::NodeId,
    profile,
    storage::ReadRepository,
};
use radicle_term as term;

use crate::terminal;

/// Hint to migrate COB database.
pub const MIGRATION_HINT: &str = "run `rad cob migrate` to update your database";

/// COB migration progress spinner.
pub struct MigrateSpinner {
    spinner: Option<term::Spinner>,
}

impl Default for MigrateSpinner {
    /// Create a new [`MigrateSpinner`].
    fn default() -> Self {
        Self { spinner: None }
    }
}

impl MigrateCallback for MigrateSpinner {
    fn progress(&mut self, progress: MigrateProgress) {
        self.spinner
            .get_or_insert_with(|| term::spinner("Migration in progress.."))
            .message(format!(
                "Migration {}/{} in progress.. ({}%)",
                progress.migration.current(),
                progress.migration.total(),
                progress.rows.percentage()
            ));

        if progress.is_done() {
            if let Some(spinner) = self.spinner.take() {
                spinner.finish()
            }
        }
    }
}

/// Migrate functions.
pub mod migrate {
    use super::MigrateSpinner;

    /// Display migration progress via a spinner.
    #[must_use]
    pub fn spinner() -> MigrateSpinner {
        MigrateSpinner::default()
    }
}

/// Return a read-only handle for the patches cache.
pub fn patches<'a, Repo>(
    profile: &Profile,
    repository: &'a Repo,
) -> Result<cob::patch::Cache<'a, Repo, ReadOnly, cob::cache::StoreReader>, anyhow::Error>
where
    Repo: ReadRepository + cob::Store<Namespace = NodeId>,
{
    profile.patches(repository).map_err(with_hint)
}

/// Return a read-write handle for the patches cache.
/// Prefer this over [`radicle::profile::Home::patches_mut`],
/// to obtain an error hint in case migrations must be run.
pub fn patches_mut<'a, 'b, Repo, Signer>(
    profile: &Profile,
    repository: &'a Repo,
    signer: &'b Signer,
) -> Result<cob::patch::Cache<'a, Repo, WriteAs<'b, Signer>, cob::cache::StoreWriter>, anyhow::Error>
where
    Repo: ReadRepository + cob::Store<Namespace = NodeId>,
{
    profile.patches_mut(repository, signer).map_err(with_hint)
}

/// Return a read-only handle for the issues cache.
pub fn issues<'a, Repo>(
    profile: &Profile,
    repository: &'a Repo,
) -> Result<cob::issue::Cache<'a, Repo, ReadOnly, cob::cache::StoreReader>, anyhow::Error>
where
    Repo: ReadRepository + cob::Store<Namespace = NodeId>,
{
    profile.issues(repository).map_err(with_hint)
}

/// Return a read-write handle for the issues cache.
/// Prefer this over [`radicle::profile::Home::issues_mut`],
/// to obtain an error hint in case migrations must be run.
pub fn issues_mut<'a, 'b, Repo, Signer>(
    profile: &Profile,
    repository: &'a Repo,
    signer: &'b Signer,
) -> Result<cob::issue::Cache<'a, Repo, WriteAs<'b, Signer>, cob::cache::StoreWriter>, anyhow::Error>
where
    Repo: ReadRepository + cob::Store<Namespace = NodeId>,
{
    profile.issues_mut(repository, signer).map_err(with_hint)
}

/// Adds a hint to the COB out-of-date database error.
fn with_hint(e: profile::Error) -> anyhow::Error {
    // There are many types that aren't `profile::Error::CobsCache`; specifying them all in an
    // error path seems overly verbose with little value.
    #[allow(clippy::wildcard_enum_match_arm)]
    match e {
        profile::Error::CobsCache(cob::cache::Error::OutOfDate) => {
            terminal::args::Error::with_hint(e, MIGRATION_HINT).into()
        }
        e => anyhow::Error::from(e),
    }
}
