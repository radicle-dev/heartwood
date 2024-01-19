use std::ffi::OsString;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};
use url::Url;

use radicle::crypto::{PublicKey, Signature, Signer};

use radicle_cli::terminal as term;
use radicle_cli::terminal::args::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "web",
    description: "Start HTTP API server and connect the web explorer to it",
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad web [<option>...] [<explorer-url>]

    Runs the Radicle HTTP Daemon and opens a Radicle web explorer to authenticate with it.

Options

    --listen, -l  <addr>     Address to bind the HTTP daemon to (default: 127.0.0.1:8080)
    --connect, -c [<addr>]   Connect the explorer to an already running daemon (default: 127.0.0.1:8080)
    --[no-]open              Open the authentication URL automatically (default: open)
    --help                   Print help
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
    pub app_url: Url,
    pub listen: SocketAddr,
    pub connect: Option<SocketAddr>,
    pub open: bool,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_args(args);
        let mut listen = None;
        let mut connect = None;
        // SAFETY: This is a valid URL.
        #[allow(clippy::unwrap_used)]
        let mut app_url = Url::parse("https://app.radicle.xyz").unwrap();
        let mut open = true;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("listen") | Short('l') if listen.is_none() => {
                    let val = parser.value()?;
                    listen = Some(term::args::socket_addr(&val)?);
                }
                Long("connect") | Short('c') if connect.is_none() => {
                    if let Ok(val) = parser.value() {
                        connect = Some(term::args::socket_addr(&val)?);
                    } else {
                        connect = Some(SocketAddr::new(
                            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                            8080,
                        ));
                    }
                }
                Long("open") => open = true,
                Long("no-open") => open = false,
                Long("help") | Short('h') => {
                    return Err(Error::Help.into());
                }
                Value(val) => {
                    let val = val.to_string_lossy();
                    app_url = Url::parse(val.as_ref()).context("invalid explorer URL supplied")?;
                }
                _ => {
                    return Err(anyhow!(arg.unexpected()));
                }
            }
        }

        Ok((
            Options {
                open,
                app_url,
                listen: listen.unwrap_or(SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    8080,
                )),
                connect,
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
    let profile = ctx.profile()?;
    let runtime_and_handle = if options.connect.is_none() {
        tracing_subscriber::fmt::init();

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to create threaded runtime");
        let httpd_handle = runtime.spawn(crate::run(crate::Options {
            aliases: Default::default(),
            listen: options.listen,
            cache: None,
        }));
        Some((runtime, httpd_handle))
    } else {
        None
    };

    let mut retries = 30;
    let connect = options.connect.unwrap_or(options.listen);
    let response = loop {
        retries -= 1;
        sleep(Duration::from_millis(100));

        match ureq::post(&format!("http://{connect}/api/v1/sessions")).call() {
            Ok(response) => {
                break response;
            }
            Err(err) => {
                if err.kind() == ureq::ErrorKind::ConnectionFailed && retries > 0 {
                    continue;
                } else {
                    anyhow::bail!(err);
                }
            }
        }
    };

    let session = response.into_json::<SessionInfo>()?;
    let signer = profile.signer()?;
    let signature = sign(signer, &session)?;

    let mut auth_url = options.app_url.clone();
    auth_url
        .path_segments_mut()
        .map_err(|_| anyhow!("URL not supported"))?
        .push("session")
        .push(&session.session_id);

    auth_url
        .query_pairs_mut()
        .append_pair("pk", &session.public_key.to_string())
        .append_pair("sig", &signature.to_string())
        .append_pair("addr", &connect.to_string());

    if options.open {
        #[cfg(target_os = "macos")]
        let cmd_name = "open";
        #[cfg(target_os = "linux")]
        let cmd_name = "xdg-open";

        let mut cmd = Command::new(cmd_name);

        match cmd.arg(auth_url.as_str()).spawn()?.wait() {
            Ok(exit_status) => {
                if exit_status.success() {
                    term::success!("Opened {auth_url}");
                } else {
                    term::info!("Visit {auth_url} to connect");
                }
            }
            Err(_) => {
                term::info!("Visit {auth_url} to connect");
            }
        }
    } else {
        term::info!("Visit {auth_url} to connect");
    }

    if let Some((runtime, httpd_handle)) = runtime_and_handle {
        runtime
            .block_on(httpd_handle)?
            .context("httpd server error")?;
    }

    Ok(())
}
