#![allow(clippy::collapsible_if)]
use std::path::PathBuf;
use std::{env, io, process};

use thiserror::Error;

use radicle::crypto::{PublicKey, Signer};
use radicle::node::Handle;
use radicle::ssh;
use radicle::storage::git::transport::{Url, UrlError};
use radicle::storage::{ReadRepository, WriteRepository, WriteStorage};

/// The service invoked by git on the remote repository, during a push.
const GIT_RECEIVE_PACK: &str = "git-receive-pack";

#[derive(Debug, Error)]
pub enum Error {
    /// Remote repository not found (or empty).
    #[error("remote repository `{0}` not found")]
    RepositoryNotFound(PathBuf),
    /// Secret key is not registered, eg. with ssh-agent.
    #[error("public key `{0}` is not registered with ssh-agent")]
    KeyNotRegistered(PublicKey),
    /// Public key doesn't match the remote namespace we're pushing to.
    #[error("public key `{0}` does not match remote namespace")]
    KeyMismatch(PublicKey),
    /// Invalid command received.
    #[error("invalid command `{0}`")]
    InvalidCommand(String),
    /// Invalid arguments received.
    #[error("invalid arguments: {0:?}")]
    InvalidArguments(Vec<String>),
    /// Error with the remote url.
    #[error("invalid remote url: {0}")]
    RemoteUrl(#[from] UrlError),
}

/// Run the radicle remote helper using the given profile.
pub fn run(profile: radicle::Profile) -> Result<(), Box<dyn std::error::Error + 'static>> {
    // `GIT_DIR` is expected to be set, though we aren't using it right now.
    let _git_dir = env::var("GIT_DIR").map(PathBuf::from)?;
    let url: Url = {
        let args = env::args().skip(1).take(2).collect::<Vec<_>>();

        match args.as_slice() {
            [url] => url.parse(),
            [_, url] => url.parse(),

            _ => {
                return Err(Error::InvalidArguments(args).into());
            }
        }
    }?;
    // Default to profile key.
    let public_key = url
        .public_key
        .unwrap_or_else(|| *profile.signer.public_key());

    let proj = profile.storage.repository(url.id)?;
    if proj.is_empty()? {
        return Err(Error::RepositoryNotFound(proj.path().to_path_buf()).into());
    }

    let stdin = io::stdin();
    loop {
        let mut line = String::new();
        let read = stdin.read_line(&mut line)?;
        if read == 0 {
            break;
        }

        let tokens = line.trim().split(' ').collect::<Vec<_>>();
        match tokens.as_slice() {
            // First we are asked about capabilities.
            ["capabilities"] => {
                println!("connect");
                println!();
            }
            // Since we send a `connect` back, this is what is requested next.
            ["connect", service] => {
                // Don't allow push if either of these conditions is true:
                //
                // 1. Our key is not in ssh-agent, which means we won't be able to sign the refs.
                // 2. Our key is not the one loaded in the profile, which means that the signed refs
                //    won't match the remote we're pushing to.
                if *service == GIT_RECEIVE_PACK {
                    if profile.signer.public_key() != &public_key {
                        return Err(Error::KeyMismatch(public_key).into());
                    }
                    if !ssh::agent::connect()?
                        .request_identities::<PublicKey>()?
                        .contains(&public_key)
                    {
                        return Err(Error::KeyNotRegistered(public_key).into());
                    }
                }
                println!(); // Empty line signifies connection is established.

                let mut child = process::Command::new(service)
                    .arg(proj.path())
                    .env("GIT_DIR", proj.path())
                    .env("GIT_NAMESPACE", public_key.to_string())
                    .stdout(process::Stdio::inherit())
                    .stderr(process::Stdio::inherit())
                    .stdin(process::Stdio::inherit())
                    .spawn()?;

                if child.wait()?.success() {
                    if *service == GIT_RECEIVE_PACK {
                        profile.storage.sign_refs(&proj, &profile.signer)?;
                        proj.set_head()?;
                        // Connect to local node and announce refs to the network.
                        // If our node is not running, we simply skip this step, as the
                        // refs will be announced eventually, when the node restarts.
                        if let Ok(conn) = profile.node() {
                            conn.announce_refs(&url.id)?;
                        }
                    }
                }
            }
            // An empty line means end of input.
            [] => {
                break;
            }
            _ => {
                return Err(Error::InvalidCommand(line).into());
            }
        }
    }

    Ok(())
}
