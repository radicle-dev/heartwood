use std::borrow::Cow;
use std::collections::BTreeSet;
use std::io;

use gix_features::progress::Progress;
use gix_protocol::handshake::Ref;
use gix_protocol::transport::Protocol;
use gix_protocol::{Handshake, ls_refs};
use gix_transport::bstr::BString;

use crate::stage::RefPrefix;

use super::{Connection, agent_name};

/// Configuration for running an ls-refs process.
///
/// See [`run`].
pub struct Config {
    /// The repository name, i.e. `/<rid>`.
    #[allow(dead_code)]
    pub repo: BString,
    /// Ref prefixes for filtering the output of the ls-refs process.
    pub prefixes: BTreeSet<RefPrefix>,
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
    handshake: &Handshake,
    mut conn: Connection<R, W>,
    progress: &mut impl Progress,
) -> Result<Vec<Ref>, ls_refs::Error>
where
    R: io::Read,
    W: io::Write,
{
    log::trace!("Performing ls-refs: {:?}", config.prefixes);
    let Handshake {
        server_protocol_version: protocol,
        capabilities,
        ..
    } = handshake;

    if protocol != &Protocol::V2 {
        return Err(ls_refs::Error::Io(io::Error::other(
            "expected protocol version 2",
        )));
    }

    let (refspecs, prefixes) = {
        let n = config.prefixes.len();
        config.prefixes.into_iter().fold(
            (Vec::with_capacity(n), Vec::with_capacity(n)),
            |(mut specs, mut prefixes), prefix| {
                specs.push(prefix.as_refspec());
                prefixes.push(prefix.into_bstring());
                (specs, prefixes)
            },
        )
    };

    log::trace!("ls-refs prefixes: {:#?}", refspecs);

    let ls_refs = gix_protocol::LsRefsCommand::new(
        Some(&refspecs),
        capabilities,
        ("agent", Some(Cow::Owned(agent_name()))),
    );

    // According to [1], in the section on `ls-refs`, we must still filter on
    // this side, since `ref-prefix` is simply an optimization.
    //
    // [1]: https://mirrors.edge.kernel.org/pub/software/scm/git/docs/gitprotocol-v2.html
    let refs = ls_refs
        .invoke_blocking(&mut conn, progress, false)?
        .into_iter()
        .filter(|r| {
            let (refname, _, _) = r.unpack();
            prefixes.iter().any(|prefix| refname.starts_with(prefix))
        })
        .collect();

    log::trace!("ls-refs received: {refs:#?}");
    Ok(refs)
}
