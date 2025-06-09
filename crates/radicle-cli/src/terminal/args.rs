use std::ffi::OsString;
use std::net::SocketAddr;
use std::str::FromStr;
use std::time;

use anyhow::anyhow;

use radicle::cob::{self, issue, patch};
use radicle::crypto;
use radicle::git::{Oid, RefString};
use radicle::node::{Address, Alias};
use radicle::prelude::{Did, NodeId, RepoId};

use crate::git::Rev;
use crate::terminal as term;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// If this error is returned from argument parsing, help is displayed.
    #[error("help invoked")]
    Help,
    /// If this error is returned from argument parsing, the manual page is displayed.
    #[error("help manual invoked")]
    HelpManual { name: &'static str },
    /// If this error is returned from argument parsing, usage is displayed.
    #[error("usage invoked")]
    Usage,
    /// An error with a hint.
    #[error("{err}")]
    WithHint {
        err: anyhow::Error,
        hint: &'static str,
    },
}

pub struct Help {
    pub name: &'static str,
    pub description: &'static str,
    pub version: &'static str,
    pub usage: &'static str,
}

impl Help {
    /// Print help to stdout.
    pub fn print(&self) {
        term::help(self.name, self.version, self.description, self.usage);
    }
}

pub trait Args: Sized {
    fn from_env() -> anyhow::Result<Self> {
        let args: Vec<_> = std::env::args_os().skip(1).collect();

        match Self::from_args(args) {
            Ok((opts, unparsed)) => {
                self::finish(unparsed)?;

                Ok(opts)
            }
            Err(err) => Err(err),
        }
    }

    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)>;
}

pub fn parse_value<T: FromStr>(flag: &str, value: OsString) -> anyhow::Result<T>
where
    <T as FromStr>::Err: std::error::Error,
{
    value
        .into_string()
        .map_err(|_| anyhow!("the value specified for '--{}' is not valid UTF-8", flag))?
        .parse()
        .map_err(|e| anyhow!("invalid value specified for '--{}' ({})", flag, e))
}

pub fn format(arg: lexopt::Arg) -> OsString {
    match arg {
        lexopt::Arg::Long(flag) => format!("--{flag}").into(),
        lexopt::Arg::Short(flag) => format!("-{flag}").into(),
        lexopt::Arg::Value(val) => val,
    }
}

pub fn finish(unparsed: Vec<OsString>) -> anyhow::Result<()> {
    if let Some(arg) = unparsed.first() {
        return Err(anyhow::anyhow!(
            "unexpected argument `{}`",
            arg.to_string_lossy()
        ));
    }
    Ok(())
}

pub fn refstring(flag: &str, value: OsString) -> anyhow::Result<RefString> {
    RefString::try_from(
        value
            .into_string()
            .map_err(|_| anyhow!("the value specified for '--{}' is not valid UTF-8", flag))?,
    )
    .map_err(|_| {
        anyhow!(
            "the value specified for '--{}' is not a valid ref string",
            flag
        )
    })
}

pub fn did(val: &OsString) -> anyhow::Result<Did> {
    let val = val.to_string_lossy();
    let Ok(peer) = Did::from_str(&val) else {
        if crypto::PublicKey::from_str(&val).is_ok() {
            return Err(anyhow!("expected DID, did you mean 'did:key:{val}'?"));
        } else {
            return Err(anyhow!("invalid DID '{}', expected 'did:key'", val));
        }
    };
    Ok(peer)
}

pub fn nid(val: &OsString) -> anyhow::Result<NodeId> {
    let val = val.to_string_lossy();
    NodeId::from_str(&val).map_err(|_| anyhow!("invalid Node ID '{}'", val))
}

pub fn rid(val: &OsString) -> anyhow::Result<RepoId> {
    let val = val.to_string_lossy();
    RepoId::from_str(&val).map_err(|_| anyhow!("invalid Repository ID '{}'", val))
}

pub fn pubkey(val: &OsString) -> anyhow::Result<NodeId> {
    let Ok(did) = did(val) else {
        let nid = nid(val)?;
        return Ok(nid);
    };
    Ok(did.as_key().to_owned())
}

pub fn socket_addr(val: &OsString) -> anyhow::Result<SocketAddr> {
    let val = val.to_string_lossy();
    SocketAddr::from_str(&val).map_err(|_| anyhow!("invalid socket address '{}'", val))
}

pub fn addr(val: &OsString) -> anyhow::Result<Address> {
    let val = val.to_string_lossy();
    Address::from_str(&val).map_err(|_| anyhow!("invalid address '{}'", val))
}

pub fn number(val: &OsString) -> anyhow::Result<usize> {
    let val = val.to_string_lossy();
    usize::from_str(&val).map_err(|_| anyhow!("invalid number '{}'", val))
}

pub fn seconds(val: &OsString) -> anyhow::Result<time::Duration> {
    let val = val.to_string_lossy();
    let secs = u64::from_str(&val).map_err(|_| anyhow!("invalid number of seconds '{}'", val))?;

    Ok(time::Duration::from_secs(secs))
}

pub fn milliseconds(val: &OsString) -> anyhow::Result<time::Duration> {
    let val = val.to_string_lossy();
    let secs =
        u64::from_str(&val).map_err(|_| anyhow!("invalid number of milliseconds '{}'", val))?;

    Ok(time::Duration::from_millis(secs))
}

pub fn string(val: &OsString) -> String {
    val.to_string_lossy().to_string()
}

pub fn rev(val: &OsString) -> anyhow::Result<Rev> {
    let s = val.to_str().ok_or(anyhow!("invalid git rev {val:?}"))?;
    Ok(Rev::from(s.to_owned()))
}

pub fn oid(val: &OsString) -> anyhow::Result<Oid> {
    let s = string(val);
    let o = radicle::git::Oid::from_str(&s).map_err(|_| anyhow!("invalid git oid '{s}'"))?;

    Ok(o)
}

pub fn alias(val: &OsString) -> anyhow::Result<Alias> {
    let val = val.as_os_str();
    let val = val
        .to_str()
        .ok_or_else(|| anyhow!("alias must be valid UTF-8"))?;

    Alias::from_str(val).map_err(|e| e.into())
}

pub fn issue(val: &OsString) -> anyhow::Result<issue::IssueId> {
    let val = val.to_string_lossy();
    issue::IssueId::from_str(&val).map_err(|_| anyhow!("invalid Issue ID '{}'", val))
}

pub fn patch(val: &OsString) -> anyhow::Result<patch::PatchId> {
    let val = val.to_string_lossy();
    patch::PatchId::from_str(&val).map_err(|_| anyhow!("invalid Patch ID '{}'", val))
}

pub fn cob(val: &OsString) -> anyhow::Result<cob::ObjectId> {
    let val = val.to_string_lossy();
    cob::ObjectId::from_str(&val).map_err(|_| anyhow!("invalid Object ID '{}'", val))
}
