use clap::Parser;

const ABOUT: &str = "List repositories";
const LONG_ABOUT: &str = r#"
By default, this command shows you all repositories that you have forked or initialized.
If you wish to see all seeded repositories, use the `--seeded` option.
"#;

#[derive(Debug, Parser)]
#[command(about = ABOUT, long_about = LONG_ABOUT, disable_version_flag = true)]
pub struct Args {
    /// Show only private repositories
    #[arg(long, conflicts_with = "public")]
    pub(super) private: bool,
    /// Show only public repositories
    #[arg(long)]
    pub(super) public: bool,
    /// Show all seeded repositories
    #[arg(short, long)]
    pub(super) seeded: bool,
    /// Show all repositories in storage
    #[arg(short, long)]
    pub(super) all: bool,
    /// Verbose output
    #[arg(short, long)]
    pub(super) verbose: bool,
}
