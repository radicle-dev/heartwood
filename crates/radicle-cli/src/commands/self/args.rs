use clap::Parser;

const ABOUT: &str = "Show information about your identity and device";

#[derive(Debug, Parser)]
#[command(about = ABOUT, disable_version_flag = true)]
#[group(multiple = false)]
pub struct Args {
    /// Show your DID
    #[arg(long)]
    pub(super) did: bool,
    /// Show your Node alias
    #[arg(long)]
    pub(super) alias: bool,
    /// Show your Node identifier
    #[arg(long, hide(true))]
    pub(super) nid: bool,
    /// Show your Radicle home
    #[arg(long)]
    pub(super) home: bool,
    /// Show the location of your configuration file
    #[arg(long)]
    pub(super) config: bool,
    /// Show your public key in OpenSSH format
    #[arg(long)]
    pub(super) ssh_key: bool,
    /// Show your public key fingerprint in OpenSSH format
    #[arg(long)]
    pub(super) ssh_fingerprint: bool,
}
