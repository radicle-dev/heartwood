pub(crate) mod fetch;
pub(crate) mod ls_refs;

use std::collections::BTreeSet;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use bstr::BString;
use gix_features::progress::prodash::progress;
use gix_protocol::Handshake;
use gix_protocol::handshake;
use gix_transport::Protocol;
use gix_transport::Service;
use gix_transport::client;
use radicle::git::Oid;
use radicle::git::fmt::Qualified;
use radicle::storage::git::Repository;
use thiserror::Error;

use crate::git::packfile::Keepfile;
use crate::git::repository;
use crate::stage::RefPrefix;

/// Open a reader and writer stream to pass to the ls-refs and fetch
/// processes for communicating during their respective protocols.
pub trait ConnectionStream {
    type Read: io::Read;
    type Write: io::Write + SignalEof;

    fn open(&mut self) -> (&mut Self::Read, &mut Self::Write);
}

/// The ability to signal EOF to the server side so that it can stop
/// serving for this fetch request.
pub trait SignalEof {
    /// Since the git protocol is tunneled over an existing
    /// connection, we can't signal the end of the protocol via the
    /// usual means, which is to close the connection. Git also
    /// doesn't have any special message we can send to signal the end
    /// of the protocol.
    ///
    /// Hence, there's no other way for the server to know that we're
    /// done sending requests than to send a special message outside
    /// the git protocol. This message can then be processed by the
    /// remote worker to end the protocol. We use the special "eof"
    /// control message for this.
    fn eof(&mut self) -> io::Result<()>;
}

/// Configuration for running a Git `handshake`, `ls-refs`, or
/// `fetch`.
pub struct Transport<S> {
    git_dir: PathBuf,
    repo: BString,
    stream: S,
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("gix ls-refs error: {0}")]
    LsRefs(#[from] gix_protocol::ls_refs::Error),
    #[error("gix fetch error: {0}")]
    Fetch(#[from] gix_protocol::fetch::Error),
    #[error("empty or no packfile received")]
    Empty,
    #[error("wanted object not found: {0}")]
    NotFound(Oid),
    #[error("gix pack index error: {0}")]
    PackIndex(#[from] gix_pack::index::init::Error),
}

impl<S> Transport<S>
where
    S: ConnectionStream,
{
    pub fn new(git_dir: PathBuf, mut repo: BString, stream: S) -> Self {
        let repo = if repo.starts_with(b"/") {
            repo
        } else {
            let mut path = BString::new(b"/".to_vec());
            path.append(&mut repo);
            path
        };
        Self {
            git_dir,
            repo,
            stream,
        }
    }

    /// Perform the handshake with the server side.
    #[allow(clippy::result_large_err)]
    pub(crate) fn handshake(&mut self) -> Result<Handshake, handshake::Error> {
        log::trace!("Performing handshake for {}", self.repo);
        let (read, write) = self.stream.open();
        gix_protocol::handshake(
            &mut Connection::new(read, write, self.repo.clone()),
            Service::UploadPack,
            |_| Ok(None),
            vec![],
            &mut progress::Discard,
        )
    }

    /// Perform ls-refs with the server side.
    pub(crate) fn ls_refs(
        &mut self,
        prefixes: impl IntoIterator<Item = RefPrefix>,
        handshake: &Handshake,
    ) -> Result<Vec<handshake::Ref>, Error> {
        let prefixes = prefixes.into_iter().collect::<BTreeSet<_>>();
        let (read, write) = self.stream.open();
        Ok(ls_refs::run(
            ls_refs::Config {
                prefixes,
                repo: self.repo.clone(),
            },
            handshake,
            Connection::new(read, write, self.repo.clone()),
            &mut progress::Discard,
        )?)
    }

    /// Perform the fetch with the server side.
    pub(crate) fn fetch(
        &mut self,
        wants_haves: WantsHaves,
        interrupt: Arc<AtomicBool>,
        handshake: &Handshake,
    ) -> Result<Option<Keepfile>, Error> {
        log::trace!(
            "Running fetch wants={:?}, haves={:?}",
            wants_haves.wants,
            wants_haves.haves
        );
        let out = {
            let (read, write) = self.stream.open();
            fetch::run(
                wants_haves.clone(),
                fetch::PackWriter {
                    git_dir: self.git_dir.clone(),
                    interrupt,
                },
                handshake,
                Connection::new(read, write, self.repo.clone()),
                &mut progress::Discard,
            )?
        };
        let pack_path = out
            .pack
            .ok_or(Error::Empty)?
            .index_path
            .expect("written packfile must have a path");

        // Validate we got all requested tips in the pack
        //
        // N.b. the lookup is a binary search so is efficient for
        // searching any given oid.
        {
            use gix_pack::index::File;

            let idx = File::at(pack_path, gix_hash::Kind::Sha1)?;
            for oid in wants_haves.wants {
                if idx.lookup(oid).is_none() {
                    return Err(Error::NotFound(oid));
                }
            }
        }

        Ok(out.keepfile)
    }

    /// Signal to the server side that we are done sending ls-refs and
    /// fetch commands.
    pub(crate) fn done(&mut self) -> io::Result<()> {
        let (_, w) = self.stream.open();
        w.eof()
    }
}

pub(crate) struct Connection<R, W> {
    inner: client::git::blocking_io::Connection<R, W>,
}

impl<R, W> Connection<R, W>
where
    R: io::Read,
    W: io::Write,
{
    pub fn new(read: R, write: W, repo: BString) -> Self {
        Self {
            inner: client::git::blocking_io::Connection::new(
                read,
                write,
                Protocol::V2,
                repo,
                None::<(String, Option<u16>)>,
                client::git::ConnectMode::Daemon,
                false,
            ),
        }
    }
}

impl<R, W> client::blocking_io::Transport for Connection<R, W>
where
    R: std::io::Read,
    W: std::io::Write,
{
    fn request(
        &mut self,
        write_mode: client::WriteMode,
        on_into_read: client::MessageKind,
        trace: bool,
    ) -> Result<client::blocking_io::RequestWriter<'_>, client::Error> {
        self.inner.request(write_mode, on_into_read, trace)
    }

    fn handshake<'b>(
        &mut self,
        service: Service,
        extra_parameters: &'b [(&'b str, Option<&'b str>)],
    ) -> Result<client::blocking_io::SetServiceResponse<'_>, client::Error> {
        self.inner.handshake(service, extra_parameters)
    }
}

impl<R, W> client::TransportWithoutIO for Connection<R, W>
where
    R: std::io::Read,
    W: std::io::Write,
{
    fn to_url(&self) -> std::borrow::Cow<'_, bstr::BStr> {
        self.inner.to_url()
    }

    fn connection_persists_across_multiple_requests(&self) -> bool {
        false
    }

    fn configure(
        &mut self,
        config: &dyn std::any::Any,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        self.inner.configure(config)
    }

    fn supported_protocol_versions(&self) -> &[Protocol] {
        &[Protocol::V2]
    }
}

#[derive(Debug, Error)]
pub enum WantsHavesError {
    #[error(transparent)]
    Ancestry(#[from] repository::error::Ancestry),
    #[error(transparent)]
    Contains(#[from] repository::error::Contains),
    #[error(transparent)]
    Resolve(#[from] repository::error::Resolve),
}

#[derive(Clone, Default)]
pub(crate) struct WantsHaves {
    pub wants: BTreeSet<Oid>,
    pub haves: BTreeSet<Oid>,
}

impl WantsHaves {
    pub fn want(&mut self, oid: Oid) {
        // N.b. if we have it, then we don't want it.
        if !self.haves.contains(&oid) {
            self.wants.insert(oid);
        }
    }

    pub fn have(&mut self, oid: Oid) {
        // N.b. ensure that oid is not in wants
        self.wants.remove(&oid);
        self.haves.insert(oid);
    }

    /// Add a set of references to the `wants` and `haves`.
    ///
    /// For each reference we want to build the range between its
    /// current `Oid` and the advertised `Oid`. This allows the server
    /// to send all objects between that range.
    ///
    /// If the reference exists, the range is given by marking the
    /// existing `Oid` as a `have` and the tip as the `want`. If the
    /// `tip`, however, is the same as the existing `Oid` or is in the
    /// Odb, then there is no need to mark it as a `want`.
    ///
    /// If the reference does not exist, the range is simply marking
    /// the tip as a `want`, iff it does not already exist in the Odb.
    pub fn add<'a, N>(
        &mut self,
        repo: &Repository,
        refs: impl IntoIterator<Item = (N, Oid)>,
    ) -> Result<&mut Self, WantsHavesError>
    where
        N: Into<Qualified<'a>>,
    {
        refs.into_iter().try_fold(self, |acc, (refname, tip)| {
            match repository::refname_to_id(repo, refname)? {
                Some(oid) => {
                    let want = oid != tip && !repository::contains(repo, tip)?;
                    acc.have(oid);

                    if want {
                        acc.want(tip)
                    }
                }
                None => {
                    if !repository::contains(repo, tip)? {
                        acc.want(tip);
                    }
                }
            };
            Ok(acc)
        })
    }
}

fn agent_name() -> String {
    let version = match radicle::git::version() {
        Ok(version) => version,
        Err(err) => {
            use radicle::git::VERSION_REQUIRED;
            log::debug!("The git version could not be determined: {err}");
            log::debug!("Pretending that we are on git version {VERSION_REQUIRED}.");
            VERSION_REQUIRED
        }
    };
    format!("git/{version}")
}
