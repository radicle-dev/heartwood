use std::path::PathBuf;

use clap::Parser;
use radicle::{
    identity::{project::ProjectName, Visibility},
    node::policy::Scope,
    prelude::RepoId,
};
use radicle_term::Interactive;

const ABOUT: &str = "Initialize a Radicle repository";

#[derive(Debug, Parser)]
#[command(about = ABOUT, disable_version_flag = true)]
pub struct Args {
    /// Directory to be initialized
    pub(super) path: Option<PathBuf>,
    /// Name of the repository
    #[arg(long)]
    pub(super) name: Option<ProjectName>,
    /// Description of the repository
    #[arg(long)]
    pub(super) description: Option<String>,
    /// The default branch of the repository
    #[arg(long = "default-branch")]
    pub(super) branch: Option<String>,
    /// Repository follow scope
    #[arg(
        long,
        default_value_t = Scope::All,
        value_name = "SCOPE",
        value_parser = ScopeParser,
    )]
    pub(super) scope: Scope,
    /// Set repository visibility to *private*
    #[arg(long, conflicts_with = "public")]
    private: bool,
    /// Set repository visibility to *public*
    #[arg(long, conflicts_with = "private")]
    public: bool,
    /// Setup repository as an existing Radicle repository
    ///
    /// [example values: rad:z3Tr6bC7ctEg2EHmLvknUr29mEDLH, z3Tr6bC7ctEg2EHmLvknUr29mEDLH]
    #[arg(long, value_name = "RID")]
    pub(super) existing: Option<RepoId>,
    /// Setup the upstream of the default branch
    #[arg(short = 'u', long)]
    pub(super) set_upstream: bool,
    /// Setup the radicle key as a signing key for this repository
    #[arg(long)]
    pub(super) setup_signing: bool,
    /// Don't ask for confirmation during setup
    #[arg(long)]
    no_confirm: bool,
    /// Don't seed this repository after initializing it
    #[arg(long)]
    no_seed: bool,
    /// Verbose mode
    #[arg(short, long)]
    pub(super) verbose: bool,
}

impl Args {
    pub(super) fn interactive(&self) -> Interactive {
        if self.no_confirm {
            Interactive::No
        } else {
            Interactive::Yes
        }
    }

    pub(super) fn visibility(&self) -> Option<Visibility> {
        if self.private {
            debug_assert!(!self.public, "BUG: `private` and `public` should conflict");
            Some(Visibility::private([]))
        } else if self.public {
            Some(Visibility::Public)
        } else {
            None
        }
    }

    pub(super) fn seed(&self) -> bool {
        !self.no_seed
    }
}

// TODO(finto): this is duplicated from `clone::args`. Consolidate these once
// the `clap` migration has finished and we can organise the shared code.
#[derive(Clone, Debug)]
struct ScopeParser;

impl clap::builder::TypedValueParser for ScopeParser {
    type Value = Scope;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        <Scope as std::str::FromStr>::from_str.parse_ref(cmd, arg, value)
    }

    fn possible_values(
        &self,
    ) -> Option<Box<dyn Iterator<Item = clap::builder::PossibleValue> + '_>> {
        use clap::builder::PossibleValue;
        Some(Box::new(
            [PossibleValue::new("all"), PossibleValue::new("followed")].into_iter(),
        ))
    }
}

#[cfg(test)]
mod test {
    use super::Args;
    use clap::error::ErrorKind;
    use clap::Parser;

    #[test]
    fn should_parse_rid_non_urn() {
        let args = Args::try_parse_from(["init", "--existing", "z3Tr6bC7ctEg2EHmLvknUr29mEDLH"]);
        assert!(args.is_ok())
    }

    #[test]
    fn should_parse_rid_urn() {
        let args =
            Args::try_parse_from(["init", "--existing", "rad:z3Tr6bC7ctEg2EHmLvknUr29mEDLH"]);
        assert!(args.is_ok())
    }

    #[test]
    fn should_not_parse_rid_url() {
        let err =
            Args::try_parse_from(["init", "--existing", "rad://z3Tr6bC7ctEg2EHmLvknUr29mEDLH"])
                .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::ValueValidation);
    }
}
