use std::io;
use std::path::PathBuf;
use std::sync::{atomic::AtomicBool, Arc};

use gix_features::progress::{DynNestedProgress, NestedProgress};
use gix_pack as pack;
use gix_protocol::fetch;
use gix_protocol::fetch::negotiate::one_round::State;
use gix_protocol::handshake;
use gix_protocol::handshake::Ref;

use crate::git::{oid, packfile};

use super::{agent_name, Connection, WantsHaves};

pub type Error = fetch::Error;

pub mod error {
    use std::io;

    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum PackWriter {
        #[error(transparent)]
        Io(#[from] io::Error),
        #[error(transparent)]
        Write(#[from] gix_pack::bundle::write::Error),
    }
}

/// Configuration for writing a packfile.
pub struct PackWriter {
    /// The repository path for writing the packfile to. Note this is
    /// the root of the Git repository, e.g. the `.git` folder.
    pub git_dir: PathBuf,
    /// `interrupt` is checked regularly and when true, the whole
    /// operation will stop.
    pub interrupt: Arc<AtomicBool>,
}

impl PackWriter {
    /// Write the packfile read from `pack` to the `objects/pack`
    /// directory.
    pub fn write_pack(
        &self,
        pack: &mut dyn std::io::BufRead,
        progress: &mut dyn DynNestedProgress,
    ) -> Result<pack::bundle::write::Outcome, error::PackWriter> {
        let options = pack::bundle::write::Options {
            // N.b. use all cores. Can make configurable if needed
            // later.
            thread_limit: None,
            iteration_mode: pack::data::input::Mode::Verify,
            index_version: pack::index::Version::V2,
            object_hash: gix_hash::Kind::Sha1,
        };
        let odb_opts = gix_odb::store::init::Options {
            slots: gix_odb::store::init::Slots::default(),
            object_hash: gix_hash::Kind::Sha1,
            use_multi_pack_index: true,
            current_dir: Some(self.git_dir.clone()),
        };
        let thickener = Arc::new(gix_odb::Store::at_opts(
            self.git_dir.join("objects"),
            &mut [].into_iter(),
            odb_opts,
        )?);
        let thickener = thickener.to_handle_arc();
        Ok(pack::Bundle::write_to_directory(
            pack,
            Some(&self.git_dir.join("objects").join("pack")),
            progress,
            &self.interrupt,
            Some(thickener),
            options,
        )?)
    }
}

/// The fetch [`Delegate`] that negotiates the fetch with the
/// server-side.
pub struct Negotiate {
    wants_haves: WantsHaves,
}

/// The result of running a fetch via [`run`].
pub struct FetchOut {
    pub refs: Vec<Ref>,
    pub pack: Option<pack::bundle::write::Outcome>,
    pub keepfile: Option<packfile::Keepfile>,
}

impl fetch::Negotiate for Negotiate {
    fn mark_complete_and_common_ref(
        &mut self,
    ) -> Result<fetch::negotiate::Action, fetch::negotiate::Error> {
        Ok(fetch::negotiate::Action::MustNegotiate {
            remote_ref_target_known: vec![],
        })
    }

    fn add_wants(
        &mut self,
        arguments: &mut fetch::Arguments,
        _remote_ref_target_known: &[bool],
    ) -> bool {
        let mut has_want = false;
        for oid in &self.wants_haves.wants {
            arguments.want(oid::to_object_id(*oid));
            has_want = true;
        }
        has_want
    }

    /// We don't actually negotiate, just provides all our haves and wants, while telling the
    /// server to make the best of it and just send a pack.
    /// Real Git negotiation can be done with calls to [`fetch::negotiate::one_round()`], but that
    /// requires a [`fetch::RefMap`] which can be instantiated with refspecs.
    fn one_round(
        &mut self,
        _state: &mut State,
        arguments: &mut fetch::Arguments,
        _previous_response: Option<&fetch::Response>,
    ) -> Result<(fetch::negotiate::Round, bool), fetch::negotiate::Error> {
        for oid in &self.wants_haves.haves {
            arguments.have(oid::to_object_id(*oid));
        }

        let round = fetch::negotiate::Round {
            haves_sent: self.wants_haves.haves.len(),
            in_vain: 0,
            haves_to_send: 0,
            previous_response_had_at_least_one_in_common: false,
        };
        let is_done = true;
        Ok((round, is_done))
    }
}

/// Run the fetch process using the provided `config` and
/// `pack_writer` configuration.
///
/// It is expected that the `handshake` was run outside of this
/// process, since it should be reused across fetch processes.
#[allow(clippy::result_large_err)]
pub(crate) fn run<P, R, W>(
    wants_haves: WantsHaves,
    pack_writer: PackWriter,
    handshake: &handshake::Outcome,
    mut conn: Connection<R, W>,
    progress: &mut P,
) -> Result<FetchOut, Error>
where
    P: NestedProgress,
    P::SubProgress: 'static,
    R: io::Read,
    W: io::Write,
{
    log::trace!(target: "fetch", "Performing fetch");

    if wants_haves.wants.is_empty() {
        return Err(Error::ReadRemainingBytes(io::Error::new(
            io::ErrorKind::InvalidData,
            "empty fetch",
        )));
    }
    let mut out = FetchOut {
        refs: Vec::new(),
        pack: None,
        keepfile: None,
    };
    let mut negotiate = Negotiate { wants_haves };
    let agent = agent_name().map_err(Error::ReadRemainingBytes)?;

    let mut pack_out = None;
    let mut handshake = handshake.clone();
    let fetch_out = gix_protocol::fetch(
        &mut negotiate,
        |read_pack, progress, _should_interrupt| -> Result<_, error::PackWriter> {
            let res = pack_writer.write_pack(read_pack, progress)?;
            pack_out = Some(res);
            Ok(true)
        },
        progress,
        &pack_writer.interrupt,
        fetch::Context {
            handshake: &mut handshake,
            transport: &mut conn,
            user_agent: ("agent", Some(agent.into())),
            trace_packetlines: false,
        },
        fetch::Options {
            shallow_file: "no shallow file required as we reject shallow remotes (and we aren't shallow ourselves)".into(),
            reject_shallow_remote: true,
            shallow: &fetch::Shallow::NoChange,
            tags: fetch::Tags::None,
        },
    )?.expect("we always get a pack");

    out.refs
        .extend(fetch_out.last_response.wanted_refs().iter().map(
            |fetch::response::WantedRef { id, path }| Ref::Direct {
                full_ref_name: path.clone(),
                object: *id,
            },
        ));
    let pack_out = pack_out.expect("we always get a pack");
    out.keepfile = pack_out
        .keep_path
        .as_ref()
        .and_then(packfile::Keepfile::new);
    out.pack = Some(pack_out);

    log::trace!(target: "fetch", "fetched refs: {:?}", out.refs);
    Ok(out)
}
