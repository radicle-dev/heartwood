use radicle::{
    cob::{
        self,
        cache::{MigrateCallback, MigrateProgress},
    },
    profile,
    storage::ReadRepository,
    Profile,
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
    pub fn spinner() -> MigrateSpinner {
        MigrateSpinner::default()
    }
}

/// Return a read-only handle for the patches cache.
pub fn patches<'a, R>(
    profile: &Profile,
    repository: &'a R,
) -> Result<cob::patch::Cache<cob::patch::Patches<'a, R>, cob::cache::StoreReader>, anyhow::Error>
where
    R: ReadRepository + cob::Store,
{
    profile.patches(repository).map_err(with_hint)
}

/// Return a read-write handle for the patches cache.
pub fn patches_mut<'a, R>(
    profile: &Profile,
    repository: &'a R,
) -> Result<cob::patch::Cache<cob::patch::Patches<'a, R>, cob::cache::StoreWriter>, anyhow::Error>
where
    R: ReadRepository + cob::Store,
{
    profile.patches_mut(repository).map_err(with_hint)
}

/// Return a read-only handle for the issues cache.
pub fn issues<'a, R>(
    profile: &Profile,
    repository: &'a R,
) -> Result<cob::issue::Cache<cob::issue::Issues<'a, R>, cob::cache::StoreReader>, anyhow::Error>
where
    R: ReadRepository + cob::Store,
{
    profile.issues(repository).map_err(with_hint)
}

/// Return a read-write handle for the issues cache.
pub fn issues_mut<'a, R>(
    profile: &Profile,
    repository: &'a R,
) -> Result<cob::issue::Cache<cob::issue::Issues<'a, R>, cob::cache::StoreWriter>, anyhow::Error>
where
    R: ReadRepository + cob::Store,
{
    profile.issues_mut(repository).map_err(with_hint)
}

/// Adds a hint to the COB out-of-date database error.
fn with_hint(e: profile::Error) -> anyhow::Error {
    match e {
        profile::Error::CobsCache(cob::cache::Error::OutOfDate) => {
            anyhow::Error::from(terminal::args::Error::WithHint {
                err: e.into(),
                hint: MIGRATION_HINT,
            })
        }
        e => anyhow::Error::from(e),
    }
}
