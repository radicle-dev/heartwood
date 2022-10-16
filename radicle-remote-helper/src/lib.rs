#![allow(clippy::collapsible_if)]
use std::path::PathBuf;
use std::str::FromStr;
use std::{env, io, process};

use thiserror::Error;

use radicle::crypto::{PublicKey, Signer};
use radicle::node::Handle;
use radicle::ssh;
use radicle::storage::{ReadRepository, WriteStorage};

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

#[derive(Debug, Error)]
pub enum UrlError {
    /// Failed to parse.
    #[error(transparent)]
    Parse(#[from] radicle::git::url::parse::Error),
    /// Unsupported URL scheme.
    #[error("{0}: unsupported scheme: expected `rad://`")]
    UnsupportedScheme(radicle::git::Url),
    /// Missing host.
    #[error("{0}: missing id")]
    MissingId(radicle::git::Url),
    /// Invalid remote repository identifier.
    #[error("{0}: id: {1}")]
    InvalidId(radicle::git::Url, radicle::identity::IdError),
    /// Invalid public key.
    #[error("{0}: key: {1}")]
    InvalidKey(radicle::git::Url, radicle::crypto::PublicKeyError),
}

/// A git remote URL.
///
/// `rad://<id>/[<pubkey>]`
///
/// Eg. `rad://zUBDc1UdoEzbpaGcNXqauQkERJ8r` without the public key,
/// and `rad://zUBDc1UdoEzbpaGcNXqauQkERJ8r/zCQTxdZGCzQXWBV3XbY3fgkHM3gfkLGyYMd2nL5R2MxQv` with.
///
#[derive(Debug)]
pub struct Url {
    pub id: radicle::identity::Id,
    pub public_key: Option<PublicKey>,
}

impl FromStr for Url {
    type Err = UrlError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let url: radicle::git::Url = s.as_bytes().try_into()?;
        Url::try_from(url)
    }
}

impl TryFrom<radicle::git::Url> for Url {
    type Error = UrlError;

    fn try_from(url: radicle::git::Url) -> Result<Self, Self::Error> {
        if url.scheme != radicle::git::url::Scheme::Radicle {
            return Err(Self::Error::UnsupportedScheme(url));
        }

        let id: radicle::identity::Id = url
            .host
            .as_ref()
            .ok_or_else(|| Self::Error::MissingId(url.clone()))?
            .parse()
            .map_err(|e| Self::Error::InvalidId(url.clone(), e))?;

        let public_key: Option<PublicKey> = if url.path.is_empty() {
            Ok(None)
        } else {
            let path = url.path.to_string();

            path.strip_prefix('/')
                .unwrap_or(&path)
                .parse()
                .map(Some)
                .map_err(|e| Self::Error::InvalidKey(url.clone(), e))
        }?;

        Ok(Url { id, public_key })
    }
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
