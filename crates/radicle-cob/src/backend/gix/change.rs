use std::collections::BTreeMap;
use std::convert::TryFrom;
use std::path::PathBuf;
use std::sync::LazyLock;

use gix::actor::Signature as Author;
use gix::bstr::ByteSlice;
use gix::objs::commit::message::body::TrailerRef;
use gix::objs::Write;
use gix::Commit;
//use git_ext::commit::{headers::Headers, Commit};
use nonempty::NonEmpty;
//use radicle_git_ext::commit::trailers::OwnedTrailer;
// type OwnedTrailer = gix::diff::object::commit::message::body::TrailerRef<'static>;

use crate::change::store::Version;
use crate::signatures;
use crate::trailers::CommitTrailer;
use crate::{
    change,
    change::{store, Contents, Timestamp},
    signatures::{ExtendedSignature, Signatures},
    trailers, Embed, Entry,
};

use oid::Oid;

/// Name of the COB manifest file.
pub const MANIFEST_BLOB_NAME: &str = "manifest";
/// Path under which COB embeds are kept.
pub const EMBEDS_PATH: LazyLock<PathBuf> = LazyLock::new(|| PathBuf::from("embeds"));

pub mod error {
    use std::str::Utf8Error;
    use std::string::FromUtf8Error;

    use crypto::ssh::ExtendedSignatureError;
    use gix::commit;
    use thiserror::Error;

    use crate::backend::gix::change::LoadContentsError;
    use crate::signatures::error::Signatures;

    use super::Oid;

    #[derive(Debug, Error)]
    pub enum Create {
        // #[error(transparent)]
        // WriteCommit(#[from] commit::error::Write),
        #[error(transparent)]
        X(#[from] gix::object::find::existing::with_conversion::Error),
        #[error(transparent)]
        FromUtf8(#[from] FromUtf8Error),
        // #[error(transparent)]
        // Git(#[from] git2::Error),
        #[error(transparent)]
        Signer(#[from] Box<dyn std::error::Error + Send + Sync + 'static>),
        #[error(transparent)]
        Signatures(#[from] Signatures),
        #[error(transparent)]
        Utf8(#[from] Utf8Error),

        #[error(transparent)]
        WriteManifest(#[from] super::WriteManifestError),

        #[error(transparent)]
        WriteCommit(#[from] gix::object::write::Error),

        #[error(transparent)]
        Lel(#[from] super::change::error::Create),
    }

    #[derive(Debug, Error)]
    pub enum Load {
        // #[error(transparent)]
        // Read(#[from] commit::error::Read),
        // #[error(transparent)]
        // Signatures(#[from] Signatures),
        // #[error(transparent)]
        // Git(#[from] git2::Error),
        #[error("a 'manifest' file was expected be found in '{0}'")]
        NoManifest(Oid),
        #[error("the 'manifest' found at '{0}' was not a blob")]
        ManifestIsNotBlob(Oid),
        #[error("the 'manifest' found at '{id}' was invalid: {err}")]
        InvalidManifest {
            id: Oid,
            #[source]
            err: serde_json::Error,
        },
        #[error("a 'change' file was expected be found in '{0}'")]
        NoChange(Oid),
        #[error("the 'change' found at '{0}' was not a blob")]
        ChangeNotBlob(Oid),
        #[error("the 'change' found at '{0}' was not signed")]
        ChangeNotSigned(Oid),
        #[error("the 'change' found at '{0}' has more than one signature")]
        TooManySignatures(Oid),
        #[error("the 'change' found at '{0}' has more than one resource trailer")]
        TooManyResources(Oid),
        #[error(transparent)]
        ResourceTrailer(#[from] super::trailers::error::InvalidResourceTrailer),
        #[error("non utf-8 characters in commit message")]
        Utf8(#[from] FromUtf8Error),

        #[error(transparent)]
        Conversion(#[from] gix::object::find::existing::with_conversion::Error),

        #[error(transparent)]
        Commit(#[from] gix::object::commit::Error),

        #[error(transparent)]
        Fooxx(#[from] gix_object::decode::Error),

        // #[error(transparent)]
        // Fxxooxx(#[from] gix::change::error::Load),
        #[error(transparent)]
        Signature(#[from] ExtendedSignatureError),

        #[error(transparent)]
        LoadContents(#[from] LoadContentsError),

        #[error(transparent)]
        Lels(#[from] gix_object::find::existing::Error),
    }
}

impl change::Storage for gix::Repository {
    type StoreError = error::Create;
    type LoadError = error::Load;

    type ObjectId = Oid;
    type Parent = Oid;
    type Signatures = ExtendedSignature;

    fn store<Signer>(
        &self,
        resource: Option<Self::Parent>,
        mut related: Vec<Self::Parent>,
        signer: &Signer,
        spec: store::Template<Self::ObjectId>,
    ) -> Result<Entry, Self::StoreError>
    where
        Signer: signature::Signer<Self::Signatures>,
    {
        let change::Template {
            type_name,
            tips,
            message,
            embeds,
            contents,
        } = spec;
        let manifest = store::Manifest::new(type_name, Version::default());
        let revision = write_manifest(self, &manifest, embeds, &contents)?;
        let tree = self.find_tree(revision)?;
        let signature = signer.sign(revision.as_ref());

        // Make sure there are no duplicates in the related list.
        related.sort();
        related.dedup();

        let (id, timestamp) = write_commit(
            self,
            // resource.map(|o| *o),
            resource,
            // Commit to tips, extra parents and resource.
            tips.iter()
                .cloned()
                .chain(related.clone())
                .chain(resource)
                .map(Oid::from),
            message,
            signature.clone(),
            related
                .iter()
                .copied()
                .map(trailers::CommitTrailer::Related),
            tree,
        )?;

        Ok(Entry {
            id,
            revision: revision.into(),
            signature,
            resource,
            parents: tips,
            related,
            manifest,
            contents,
            timestamp,
        })
    }

    fn parents_of(&self, id: &Oid) -> Result<Vec<Oid>, Self::LoadError> {
        Ok(self
            .find_commit(*id)?
            .parent_ids()
            .map(Oid::from)
            .collect::<Vec<_>>())
    }

    fn manifest_of(&self, id: &Oid) -> Result<crate::Manifest, Self::LoadError> {
        let commit = self.find_commit(*id)?;
        let tree = commit.tree()?;
        load_manifest(&tree)
    }

    fn load(&self, id: Self::ObjectId) -> Result<Entry, Self::LoadError> {
        // let commit = Commit::read(self, id.into())?;
        let commit = self.find_commit(id)?;
        // let timestamp = git2::Time::from(commit.committer().time).seconds() as u64;
        let timestamp = commit.time()?.seconds as u64;
        let trailers = commit
            .message()?
            .body()
            .map(|body| body.trailers().into_iter());
        let trailers = parse_trailers(trailers.into_iter().flatten())?;
        let (resources, related): (Vec<_>, Vec<_>) = trailers.into_iter().partition(|t| match t {
            CommitTrailer::Resource(_) => true,
            CommitTrailer::Related(_) => false,
        });
        let mut resources = resources.into_iter().map(|r| r.into()).collect::<Vec<_>>();
        let related = related.into_iter().map(|r| r.into()).collect::<Vec<_>>();
        let parents = commit
            .parent_ids()
            .map(Oid::from)
            .filter(|p| !resources.contains(p) && !related.contains(p))
            .collect();
        let Some((signature, _data)) = commit.signature()? else {
            return Err(error::Load::ChangeNotSigned(id));
        };
        if resources.len() > 1 {
            return Err(error::Load::TooManyResources(id));
        };

        let tree = commit.tree()?;
        let manifest = load_manifest(&tree)?;
        let contents = load_contents(&tree)?;

        Ok(Entry {
            id,
            revision: tree.id().into(),
            signature: ExtendedSignature::from_pem(signature.as_bytes())?,
            resource: resources.pop(),
            related,
            parents,
            manifest,
            contents,
            timestamp,
        })
    }
}

fn parse_trailers<'a>(
    trailers: impl Iterator<Item = TrailerRef<'a>>,
) -> Result<Vec<trailers::CommitTrailer>, error::Load> {
    let mut parsed = Vec::new();
    for trailer in trailers {
        match trailers::CommitTrailer::try_from(trailer) {
            Err(trailers::error::InvalidResourceTrailer::WrongToken) => {
                continue;
            }
            Err(err) => return Err(err.into()),
            Ok(t) => parsed.push(t),
        }
    }
    Ok(parsed)
}

fn load_manifest(tree: &gix::Tree) -> Result<store::Manifest, error::Load> {
    let Some(manifest_tree_entry) = tree.lookup_entry_by_path(MANIFEST_BLOB_NAME)? else {
        return Err(error::Load::NoManifest(tree.id().into()))?;
    };
    let manifest_object = manifest_tree_entry.object()?;
    let manifest_blob = manifest_object
        .try_into_blob()
        .map_err(|_| error::Load::ManifestIsNotBlob(tree.id().into()))?;

    serde_json::from_slice(&manifest_blob.data).map_err(|err| error::Load::InvalidManifest {
        id: tree.id().into(),
        err,
    })
}

#[derive(Debug, thiserror::Error)]
pub enum LoadContentsError {
    #[error(transparent)]
    Decode(#[from] gix_object::decode::Error),
    #[error(transparent)]
    Find(#[from] gix::object::find::existing::Error),

    #[error("a 'change' file was expected be found in '{0}'")]
    NoChange(Oid),
}

fn load_contents(tree: &gix::Tree) -> Result<Contents, LoadContentsError> {
    let mut map: BTreeMap<i8, Vec<u8>> = BTreeMap::new();

    for entry in tree.iter() {
        let entry = entry?;
        if entry.kind() != gix::objs::tree::EntryKind::Blob {
            continue;
        }

        let Ok(name) = entry.filename().to_str_lossy().parse::<i8>() else {
            continue;
        };
        let blob = entry.object()?.into_blob().take_data();

        map.insert(name, blob);
    }

    NonEmpty::collect(map.into_values())
        .ok_or_else(|| LoadContentsError::NoChange(tree.id().into()))
}

fn write_commit(
    repo: &gix::Repository,
    resource: Option<Oid>,
    parents: impl IntoIterator<Item = Oid>,
    message: String,
    signature: ExtendedSignature,
    trailers: impl IntoIterator<Item = trailers::CommitTrailer>,
    tree: gix::Tree,
) -> Result<(Oid, Timestamp), error::Create> {
    let trailers: Vec<trailers::CommitTrailer> = trailers
        .into_iter()
        .chain(resource.map(trailers::CommitTrailer::Resource))
        .collect();

    let author = repo.author().unwrap().unwrap().to_owned().unwrap();
    #[allow(unused_variables)]
    let timestamp = author.time.seconds;

    let mut headers: Vec<(gix::bstr::BString, gix::bstr::BString)> = vec![(
        "gpgsig".into(),
        signature
            .to_pem()
            .map_err(signatures::error::Signatures::from)?
            .into(),
    )];

    // let author = Author::try_from(&author)?;

    #[cfg(feature = "stable-commit-ids")]
    // Ensures the commit id doesn't change on every run.
    let (author, timestamp) = {
        let stable = crate::backend::stable::read_timestamp();
        (
            Author {
                time: gix::date::Time::new(stable, 0),
                ..author
            },
            stable,
        )
    };
    let (author, timestamp) = if let Ok(s) = std::env::var(super::GIT_COMMITTER_DATE) {
        let Ok(timestamp) = s.trim().parse::<i64>() else {
            panic!(
                "Invalid timestamp value {s:?} for `{}`",
                super::GIT_COMMITTER_DATE
            );
        };
        let author = Author {
            time: gix::date::Time::new(timestamp, 0),
            ..author
        };
        (author, timestamp)
    } else {
        (author, timestamp)
    };

    let committer = author.clone();

    let commit = gix_object::Commit {
        encoding: None,
        message: "lel".into(),
        tree: tree.id,
        extra_headers: headers,
        author: author,
        committer,
        parents: parents.into_iter().map(gix::ObjectId::from).collect(),
    };

    Ok((repo.write_object(commit)?.into(), timestamp as u64))
}

#[derive(Debug, thiserror::Error)]
pub enum WriteManifestError {
    #[error(transparent)]
    Write(#[from] gix_object::write::Error),

    #[error(transparent)]
    Foo(#[from] gix_object::tree::editor::Error),

    #[error(transparent)]
    Lelzo(#[from] gix::object::tree::editor::write::Error),

    #[error(transparent)]
    Huhu(#[from] gix::repository::edit_tree::Error),

    #[error(transparent)]
    Abc(#[from] gix::object::write::Error),
}

fn write_manifest(
    repo: &gix::Repository,
    manifest: &store::Manifest,
    embeds: Vec<Embed<Oid>>,
    contents: &NonEmpty<Vec<u8>>,
) -> Result<Oid, WriteManifestError> {
    let mut root = repo.edit_tree(gix::ObjectId::empty_tree(gix::hash::Kind::Sha1))?;

    // Insert manifest file into tree.
    {
        // SAFETY: we're serializing to an in memory buffer so the only source of
        // errors here is a programming error, which we can't recover from.
        #[allow(clippy::unwrap_used)]
        let manifest = serde_json::to_vec(manifest).unwrap();
        let manifest_oid = repo.write_blob(manifest)?;

        root.upsert(
            MANIFEST_BLOB_NAME,
            gix::objs::tree::EntryKind::Blob,
            manifest_oid,
        )?;
    }

    // Insert each COB entry.
    for (ix, op) in contents.iter().enumerate() {
        let oid = repo.write_blob(op)?;
        root.upsert(ix.to_string(), gix::objs::tree::EntryKind::Blob, oid)?;
    }

    // Insert each embed in a tree at `/embeds`.
    if !embeds.is_empty() {
        let mut embeds_tree = repo.edit_tree(gix::ObjectId::empty_tree(gix::hash::Kind::Sha1))?;

        for embed in embeds {
            embeds_tree.upsert(embed.name, gix::objs::tree::EntryKind::Blob, embed.content)?;
        }
        let oid = embeds_tree.write()?;

        root.upsert("embeds", gix::objs::tree::EntryKind::Tree, oid)?;
    }
    let oid = root.write()?;

    Ok(oid.into())
}
