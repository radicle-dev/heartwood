// Copyright © 2022 The Radicle Link Contributors

use std::collections::BTreeMap;
use std::convert::TryFrom;

use git_ext::author::Author;
use git_ext::commit::{headers::Headers, Commit};
use git_ext::Oid;
use nonempty::NonEmpty;
use radicle_git_ext::commit::trailers::OwnedTrailer;

use crate::history::entry::Timestamp;
use crate::signatures;
use crate::{
    change::{self, store, Change},
    history::entry,
    signatures::{ExtendedSignature, Signatures},
    trailers,
};

const MANIFEST_BLOB_NAME: &str = "manifest";

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
        resource: Self::Parent,
        parents: Vec<Self::Parent>,
        signer: &Signer,
        spec: store::Template<Self::ObjectId>,
    ) -> Result<Change, Self::StoreError>
    where
        Signer: crypto::Signer,
    {
        let change::Template {
            typename,
            history_type,
            tips,
            message,
            contents,
        } = spec;
        let manifest = store::Manifest {
            typename,
            history_type,
        };

        let revision = write_manifest(self, &manifest, &contents)?;
        let tree = self.find_tree(revision)?;
        let signature = {
            let sig = signer.sign(revision.as_bytes());
            let key = signer.public_key();
            ExtendedSignature::new(*key, sig)
        };

        let (id, timestamp) = write_commit(
            self,
            resource,
            parents.clone(),
            tips,
            message,
            signature.clone(),
            tree,
        )?;
        Ok(Change {
            id,
            revision: revision.into(),
            signature,
            resource,
            parents,
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

    fn load(&self, id: Self::ObjectId) -> Result<Change, Self::LoadError> {
        let commit = Commit::read(self, id.into())?;
        let timestamp = git2::Time::from(commit.committer().time).seconds() as u64;
        let resource = parse_resource_trailer(commit.trailers())?;
        let parents = commit
            .parents()
            .map(Oid::from)
            .filter(|p| *p != resource)
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

        let tree = self.find_tree(commit.tree())?;
        let manifest = load_manifest(self, &tree)?;
        let contents = load_contents(self, &tree)?;

        Ok(Change {
            id,
            revision: tree.id().into(),
            signature: ExtendedSignature::new(key, sig),
            resource,
            parents,
            manifest,
            contents,
            timestamp,
        })
    }
}

fn parse_resource_trailer<'a>(
    trailers: impl Iterator<Item = &'a OwnedTrailer>,
) -> Result<Oid, error::Load> {
    for trailer in trailers {
        match trailers::ResourceCommitTrailer::try_from(trailer) {
            Err(trailers::error::InvalidResourceTrailer::WrongToken) => {
                continue;
            }
            Err(err) => return Err(err.into()),
            Ok(resource) => return Ok(resource.oid().into()),
        }
    }
    Err(error::Load::from(
        trailers::error::InvalidResourceTrailer::NoTrailer,
    ))
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

fn load_contents(
    repo: &git2::Repository,
    tree: &git2::Tree,
) -> Result<entry::Contents, error::Load> {
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

fn write_commit<O>(
    repo: &git2::Repository,
    resource: O,
    parents: Vec<O>,
    tips: Vec<O>,
    message: String,
    signature: ExtendedSignature,
    tree: git2::Tree,
) -> Result<(Oid, Timestamp), error::Create>
where
    O: AsRef<git2::Oid>,
{
    let resource = *resource.as_ref();
    // Add extra parents ensuring there are no duplicates.
    let mut parents = parents.iter().map(|o| *o.as_ref()).collect::<Vec<_>>();
    parents.sort();
    parents.dedup();

    let parents = tips
        .iter()
        .map(|o| *o.as_ref())
        .chain(parents.into_iter())
        .chain(std::iter::once(resource))
        .collect::<Vec<_>>();

    let trailers: Vec<OwnedTrailer> = vec![trailers::ResourceCommitTrailer::from(resource).into()];
    let author = repo.signature()?;
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

    #[cfg(debug_assertions)]
    let (author, timestamp) = if let Ok(s) = std::env::var(crate::git::RAD_COMMIT_TIME) {
        let timestamp = s.trim().parse::<i64>().unwrap();
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
    contents: &NonEmpty<Vec<u8>>,
) -> Result<git2::Oid, git2::Error> {
    let mut tb = repo.treebuilder(None)?;
    // SAFETY: we're serializing to an in memory buffer so the only source of
    // errors here is a programming error, which we can't recover from
    let serialized_manifest = serde_json::to_vec(manifest).unwrap();
    let manifest_oid = repo.blob(&serialized_manifest)?;
    tb.insert(
        MANIFEST_BLOB_NAME,
        manifest_oid,
        git2::FileMode::Blob.into(),
    )?;

    for (ix, op) in contents.iter().enumerate() {
        let oid = repo.blob(op.as_ref())?;
        tb.insert(&ix.to_string(), oid, git2::FileMode::Blob.into())?;
    }
    let tree_oid = tb.write()?;

    Ok(tree_oid)
}
