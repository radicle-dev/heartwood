use std::borrow::Cow;
use std::io;

use gix_features::progress::Progress;
use gix_protocol::handshake::{self, Ref};
use gix_protocol::ls_refs;
use gix_protocol::transport::Protocol;
use gix_transport::bstr::{BString, ByteVec};

use super::{agent_name, Connection};

/// Configuration for running an ls-refs process.
///
/// See [`run`].
pub struct Config {
    /// The repository name, i.e. `/<rid>`.
    #[allow(dead_code)]
    pub repo: BString,
    /// Ref prefixes for filtering the output of the ls-refs process.
    pub prefixes: Vec<BString>,
}

/// Run the ls-refs process using the provided `config`.
///
/// It is expected that the `handshake` was run outside of this
/// process, since it should be reused across fetch processes.
///
/// The resulting set of references are the ones returned by the
/// ls-refs process, filtered by any prefixes that were provided by
/// the `config`.
pub(crate) fn run<R, W>(
    config: Config,
    handshake: &handshake::Outcome,
    mut conn: Connection<R, W>,
    progress: &mut impl Progress,
) -> Result<Vec<Ref>, ls_refs::Error>
where
    R: io::Read,
    W: io::Write,
{
    log::trace!(target: "fetch", "Performing ls-refs: {:?}", config.prefixes);
    let handshake::Outcome {
        server_protocol_version: protocol,
        capabilities,
        ..
    } = handshake;

    if protocol != &Protocol::V2 {
        return Err(ls_refs::Error::Io(io::Error::new(
            io::ErrorKind::Other,
            "expected protocol version 2",
        )));
    }

    let refs = gix_protocol::ls_refs(
        &mut conn,
        capabilities,
        |_caps, args, features| {
            for prefix in &config.prefixes {
                let mut arg = BString::from("ref-prefix ");
                arg.push_str(prefix);
                args.push(arg)
            }
            features.push(("agent", Some(Cow::Owned(agent_name()?))));
            Ok(gix_protocol::ls_refs::Action::Continue)
        },
        progress,
        false, /* trace packetlines */
    )?;

    Ok(refs)
}
