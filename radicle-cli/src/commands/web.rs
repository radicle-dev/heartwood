use std::ffi::OsString;

use anyhow::anyhow;
use radicle::crypto::{PublicKey, Signature, Signer};
use serde::{Deserialize, Serialize};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "web",
    description: "Connect web with node",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad web [<options>...]

Options

    --host, -h             httpd host to bind to
    --web, -w              interface host to bind to
    --verbose, -v          Verbose output
    --help                 Print help
"#,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub session_id: String,
    pub public_key: PublicKey,
}

#[derive(Debug)]
pub struct Options {
    pub host: String,
    pub web: String,
    pub verbose: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut host = None;
        let mut web = None;
        let mut verbose = false;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("verbose") | Short('v') => verbose = true,
                Long("host") | Short('h') => {
                    host = Some(parser.value()?.to_string_lossy().to_string())
                }
                Long("web") | Short('w') => {
                    web = Some(parser.value()?.to_string_lossy().to_string())
                }
                Long("help") => {
                    return Err(Error::Help.into());
                }
                _ => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        Ok((
            Options {
                verbose,
                host: host.unwrap_or(String::from("0.0.0.0:8080")),
                web: web.unwrap_or(String::from("localhost:3000")),
            },
            vec![],
        ))
    }
}

pub fn sign(signer: Box<dyn Signer>, session: &SessionInfo) -> Result<Signature, anyhow::Error> {
    signer
        .try_sign(format!("{}:{}", session.session_id, session.public_key).as_bytes())
        .map_err(anyhow::Error::from)
}

pub fn run(options: Options, ctx: impl term::Context) -> anyhow::Result<()> {
    let session: SessionInfo = ureq::post(&format!("http://{}/api/v1/sessions", options.host))
        .call()?
        .into_json()?;
    let profile = ctx.profile()?;
    let signer = profile.signer()?;
    let signature = sign(signer, &session)?;
    term::info!(
        "http://{}/session/{}?pk={}&sig={}",
        options.web,
        session.session_id,
        session.public_key,
        signature,
    );

    Ok(())
}
