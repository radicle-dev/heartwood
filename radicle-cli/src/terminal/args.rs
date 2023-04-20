use std::ffi::OsString;
use std::str::FromStr;

use anyhow::anyhow;

use radicle::cob::{self, issue, patch};
use radicle::crypto;
use radicle::node::Address;
use radicle::prelude::{Did, Id, NodeId};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// If this error is returned from argument parsing, help is displayed.
    #[error("help invoked")]
    Help,
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

pub trait Args: Sized {
    fn from_env() -> anyhow::Result<Self> {
        let args: Vec<_> = std::env::args_os().into_iter().skip(1).collect();

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
        .map_err(|_| anyhow!("the value specified for '--{}' is not valid unicode", flag))?
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

pub fn rid(val: &OsString) -> anyhow::Result<Id> {
    let val = val.to_string_lossy();
    Id::from_str(&val).map_err(|_| anyhow!("invalid Repository ID '{}'", val))
}

pub fn pubkey(val: &OsString) -> anyhow::Result<NodeId> {
    let Ok(did) = did(val) else {
        let nid = nid(val)?;
        return Ok(nid)
    };
    Ok(did.as_key().to_owned())
}

pub fn addr(val: &OsString) -> anyhow::Result<Address> {
    let val = val.to_string_lossy();
    Address::from_str(&val).map_err(|_| anyhow!("invalid address '{}'", val))
}

pub fn string(val: &OsString) -> String {
    val.to_string_lossy().to_string()
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
