use clap::Parser;
use radicle::identity::PlcId;
use radicle::node::Alias;

const ABOUT: &str = "Manage identities and profiles";
const LONG_ABOUT: &str = r#"
A passphrase may be given via the environment variable `RAD_PASSPHRASE` or
via the standard input stream if `--stdin` is used. Using either of these
methods disables the passphrase prompt.

Link an ATProto PLC DID to this device with `--link-plc did:plc:…`. The local
Ed25519 key must appear as a Multikey `#atproto` or `#radicle` verification
method on that DID document.
"#;

#[derive(Debug, Parser)]
#[command(about = ABOUT, long_about = LONG_ABOUT, disable_version_flag = true)]
pub struct Args {
    /// When initializing an identity, sets the node alias
    #[arg(long)]
    pub alias: Option<Alias>,

    /// Read passphrase from stdin
    #[arg(long, default_value_t = false)]
    pub stdin: bool,

    /// Link this device to an ATProto `did:plc` identity
    #[arg(long, value_name = "DID")]
    pub link_plc: Option<PlcId>,
}
