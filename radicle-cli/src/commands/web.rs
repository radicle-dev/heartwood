use std::ffi::OsString;

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

use radicle::crypto::{PublicKey, Signature, Signer};

use crate::terminal as term;
use crate::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "web",
    description: "Connect web with node",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad web [<option>...]

Options

    --backend, -b          httpd to bind to
    --frontend, -f         Web interface to bind to
    --verbose, -v          Verbose output
    --json                 Output as json
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
    pub backend: String,
    pub frontend: String,
    pub json: bool,
    pub verbose: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut backend = None;
        let mut frontend = None;
        let mut json = false;
        let mut verbose = false;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("verbose") | Short('v') => verbose = true,
                Long("backend") | Short('b') => {
                    backend = Some(parser.value()?.to_string_lossy().to_string())
                }
                Long("frontend") | Short('f') => {
                    frontend = Some(parser.value()?.to_string_lossy().to_string())
                }
                Long("json") => json = true,
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                _ => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        Ok((
            Options {
                json,
                verbose,
                backend: backend.unwrap_or(String::from("http://0.0.0.0:8080")),
                frontend: frontend.unwrap_or(String::from("https://app.radicle.xyz")),
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
    let session: SessionInfo = ureq::post(&format!("{}/api/v1/sessions", options.backend))
        .call()?
        .into_json()?;
    let profile = ctx.profile()?;
    let signer = profile.signer()?;
    let signature = sign(signer, &session)?;

    if options.json {
        println!(
            "{}",
            serde_json::json!({
                "sessionId": session.session_id,
                "publicKey": session.public_key,
                "signature": signature,
            })
        )
    } else {
        term::blank();
        term::info!("Open the following link to authenticate:");
        term::info!(
            "  👉 {}/session/{}?pk={}&sig={}",
            options.frontend,
            session.session_id,
            session.public_key,
            signature,
        );
        term::blank();
    }

    Ok(())
}
