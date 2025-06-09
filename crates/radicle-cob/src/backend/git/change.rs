// Copyright © 2022 The Radicle Link Contributors

use std::collections::BTreeMap;
use std::convert::TryFrom;
use std::path::PathBuf;
use std::sync::LazyLock;

use git_ext::author::Author;
use git_ext::commit::{headers::Headers, Commit};
use git_ext::Oid;
use nonempty::NonEmpty;
use radicle_git_ext::commit::trailers::OwnedTrailer;

use crate::change::store::Version;
use crate::signatures;
use crate::trailers::CommitTrailer;
use crate::{
    change,
    change::{store, Contents, Entry, Timestamp},
    signatures::{ExtendedSignature, Signatures},
    trailers, Embed,
};

/// Name of the COB manifest file.
pub const MANIFEST_BLOB_NAME: &str = "manifest";
/// Path under which COB embeds are kept.
pub static EMBEDS_PATH: LazyLock<PathBuf> = LazyLock::new(|| PathBuf::from("embeds"));

pub mod error {
    use std::str::Utf8Error;
    use std::string::FromUtf8Error;

    use git_ext::commit;
    use git_ext::Oid;
    use thiserror::Error;

    use crate::signatures::error::Signatures;

    #[derive(Debug, Error)]
    pub enum Create {
        #[error(transparent)]
        WriteCommit(#[from] commit::error::Write),
        #[error(transparent)]
        FromUtf8(#[from] FromUtf8Error),
        #[error(transparent)]
        Git(#[from] git2::Error),
        #[error(transparent)]
        Signer(#[from] Box<dyn std::error::Error + Send + Sync + 'static>),
        #[error(transparent)]
        Signatures(#[from] Signatures),
        #[error(transparent)]
        Utf8(#[from] Utf8Error),
    }

    #[derive(Debug, Error)]
    pub enum Load {
        #[error(transparent)]
        Read(#[from] commit::error::Read),
        #[error(transparent)]
        Signatures(#[from] Signatures),
        #[error(transparent)]
        Git(#[from] git2::Error),
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
    }
}

impl change::Storage for git2::Repository {
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
        let signature = signer.sign(revision.as_bytes());

        // Make sure there are no duplicates in the related list.
        related.sort();
        related.dedup();

        let (id, timestamp) = write_commit(
            self,
            resource.map(|o| *o),
            // Commit to tips, extra parents and resource.
            tips.iter()
                .cloned()
                .chain(related.clone())
                .chain(resource)
                .map(git2::Oid::from),
            message,
            signature.clone(),
            related
                .iter()
                .map(|p| trailers::CommitTrailer::Related(**p).into()),
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
            .find_commit(**id)?
            .parent_ids()
            .map(Oid::from)
            .collect::<Vec<_>>())
    }

    fn load(&self, id: Self::ObjectId) -> Result<Entry, Self::LoadError> {
        let commit = Commit::read(self, id.into())?;
        let timestamp = git2::Time::from(commit.committer().time).seconds() as u64;
        let trailers = parse_trailers(commit.trailers())?;
        let (resources, related): (Vec<_>, Vec<_>) = trailers.iter().partition(|t| match t {
            CommitTrailer::Resource(_) => true,
            CommitTrailer::Related(_) => false,
        });
        let mut resources = resources
            .into_iter()
            .map(|r| r.oid().into())
            .collect::<Vec<_>>();
        let related = related
            .into_iter()
            .map(|r| r.oid().into())
            .collect::<Vec<_>>();
        let parents = commit
            .parents()
            .map(Oid::from)
            .filter(|p| !resources.contains(p) && !related.contains(p))
            .collect();
        let mut signatures = Signatures::try_from(&commit)?
            .into_iter()
            .collect::<Vec<_>>();
        let Some((key, sig)) = signatures.pop() else {
            return Err(error::Load::ChangeNotSigned(id));
        };
        if !signatures.is_empty() {
            return Err(error::Load::TooManySignatures(id));
        }
        if resources.len() > 1 {
            return Err(error::Load::TooManyResources(id));
        };

        let tree = self.find_tree(*commit.tree())?;
        let manifest = load_manifest(self, &tree)?;
        let contents = load_contents(self, &tree)?;

        Ok(Entry {
            id,
            revision: tree.id().into(),
            signature: ExtendedSignature::new(key, sig),
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
    trailers: impl Iterator<Item = &'a OwnedTrailer>,
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

fn load_manifest(
    repo: &git2::Repository,
    tree: &git2::Tree,
) -> Result<store::Manifest, error::Load> {
    let manifest_tree_entry = tree
        .get_name(MANIFEST_BLOB_NAME)
        .ok_or_else(|| error::Load::NoManifest(tree.id().into()))?;
    let manifest_object = manifest_tree_entry.to_object(repo)?;
    let manifest_blob = manifest_object
        .as_blob()
        .ok_or_else(|| error::Load::ManifestIsNotBlob(tree.id().into()))?;

    serde_json::from_slice(manifest_blob.content()).map_err(|err| error::Load::InvalidManifest {
        id: tree.id().into(),
        err,
    })
}

fn load_contents(repo: &git2::Repository, tree: &git2::Tree) -> Result<Contents, error::Load> {
    let ops = tree
        .iter()
        .filter_map(|entry| {
            entry.kind().and_then(|kind| match kind {
                git2::ObjectType::Blob => {
                    let name = entry.name()?.parse::<i8>().ok()?;
                    let blob = entry
                        .to_object(repo)
                        .and_then(|object| object.peel_to_blob())
                        .map(|blob| blob.content().to_owned())
                        .map(|b| (name, b));

                    Some(blob)
                }
                _ => None,
            })
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;

    NonEmpty::collect(ops.into_values()).ok_or_else(|| error::Load::NoChange(tree.id().into()))
}

fn write_commit(
    repo: &git2::Repository,
    resource: Option<git2::Oid>,
    parents: impl IntoIterator<Item = git2::Oid>,
    message: String,
    signature: ExtendedSignature,
    trailers: impl IntoIterator<Item = OwnedTrailer>,
    tree: git2::Tree,
) -> Result<(Oid, Timestamp), error::Create> {
    let trailers: Vec<OwnedTrailer> = trailers
        .into_iter()
        .chain(resource.map(|r| trailers::CommitTrailer::Resource(r).into()))
        .collect();
    let author = repo.signature()?;
    #[allow(unused_variables)]
    let timestamp = author.when().seconds();

    let mut headers = Headers::new();
    headers.push(
        "gpgsig",
        signature
            .to_pem()
            .map_err(signatures::error::Signatures::from)?
            .as_str(),
    );
    let author = Author::try_from(&author)?;

    #[cfg(feature = "stable-commit-ids")]
    // Ensures the commit id doesn't change on every run.
    let (author, timestamp) = {
        let stable = crate::git::stable::read_timestamp();
        (
            Author {
                time: git_ext::author::Time::new(stable, 0),
                ..author
            },
            stable,
        )
    };
    let (author, timestamp) = if let Ok(s) = std::env::var(crate::git::GIT_COMMITTER_DATE) {
        let Ok(timestamp) = s.trim().parse::<i64>() else {
            panic!(
                "Invalid timestamp value {s:?} for `{}`",
                crate::git::GIT_COMMITTER_DATE
            );
        };
        let author = Author {
            time: git_ext::author::Time::new(timestamp, 0),
            ..author
        };
        (author, timestamp)
    } else {
        (author, timestamp)
    };

    let oid = Commit::new(
        tree.id(),
        parents,
        author.clone(),
        author,
        headers,
        message,
        trailers,
    )
    .write(repo)?;

    Ok((Oid::from(oid), timestamp as u64))
}

fn write_manifest(
    repo: &git2::Repository,
    manifest: &store::Manifest,
    embeds: Vec<Embed<Oid>>,
    contents: &NonEmpty<Vec<u8>>,
) -> Result<git2::Oid, git2::Error> {
    let mut root = repo.treebuilder(None)?;

    // Insert manifest file into tree.
    {
        // SAFETY: we're serializing to an in memory buffer so the only source of
        // errors here is a programming error, which we can't recover from.
        #[allow(clippy::unwrap_used)]
        let manifest = serde_json::to_vec(manifest).unwrap();
        let manifest_oid = repo.blob(&manifest)?;

        root.insert(
            MANIFEST_BLOB_NAME,
            manifest_oid,
            git2::FileMode::Blob.into(),
        )?;
    }

    // Insert each COB entry.
    for (ix, op) in contents.iter().enumerate() {
        let oid = repo.blob(op.as_ref())?;
        root.insert(ix.to_string(), oid, git2::FileMode::Blob.into())?;
    }

    // Insert each embed in a tree at `/embeds`.
    if !embeds.is_empty() {
        let mut embeds_tree = repo.treebuilder(None)?;

        for embed in embeds {
            let oid = embed.content;
            let path = PathBuf::from(embed.name);

            embeds_tree.insert(path, *oid, git2::FileMode::Blob.into())?;
        }
        let oid = embeds_tree.write()?;

        root.insert(&*EMBEDS_PATH, oid, git2::FileMode::Tree.into())?;
    }
    let oid = root.write()?;

    Ok(oid)
}
