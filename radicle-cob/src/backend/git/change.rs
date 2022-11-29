// Copyright © 2022 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use std::convert::TryFrom;

use git_commit::{self as commit, Commit};
use git_ext::Oid;
use git_trailers::OwnedTrailer;

use crate::{
    change::{self, store, Change},
    history::entry,
    signatures::{Signature, Signatures},
    trailers, HistoryType,
};

const MANIFEST_BLOB_NAME: &str = "manifest";
const CHANGE_BLOB_NAME: &str = "change";

pub mod error {
    use std::str::Utf8Error;
    use std::string::FromUtf8Error;

    use git_ext::Oid;
    use git_trailers::Error as TrailerError;
    use thiserror::Error;

    use crate::signatures::error::Signatures;
    use crate::trailers;

    #[derive(Debug, Error)]
    pub enum Create {
        #[error(transparent)]
        FromUtf8(#[from] FromUtf8Error),
        #[error(transparent)]
        Git(#[from] git2::Error),
        #[error(transparent)]
        Signer(#[from] Box<dyn std::error::Error + Send + Sync + 'static>),
        #[error(transparent)]
        Utf8(#[from] Utf8Error),
    }

    #[derive(Debug, Error)]
    pub enum Load {
        #[error(transparent)]
        Read(#[from] git_commit::error::Read),
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
        AuthorTrailer(#[from] trailers::error::InvalidAuthorTrailer),
        #[error(transparent)]
        ResourceTrailer(#[from] super::trailers::error::InvalidResourceTrailer),
        #[error("non utf-8 characters in commit message")]
        Utf8(#[from] FromUtf8Error),
        #[error(transparent)]
        Trailer(#[from] TrailerError),
    }
}

impl change::Storage for git2::Repository {
    type CreateError = error::Create;
    type LoadError = error::Load;

    type ObjectId = Oid;
    type Author = Oid;
    type Resource = Oid;
    type Signatures = Signature;

    fn create<Signer>(
        &self,
        author: Option<Self::Author>,
        resource: Self::Resource,
        signer: &Signer,
        spec: store::Create<Self::ObjectId>,
    ) -> Result<Change, Self::CreateError>
    where
        Signer: crypto::Signer,
    {
        let change::Create {
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
            Signature::from((*key, sig))
        };

        let id = write_commit(
            self,
            author,
            resource,
            tips,
            message,
            signature.clone(),
            tree,
        )?;
        Ok(Change {
            id,
            revision: revision.into(),
            signature,
            author,
            resource,
            manifest,
            contents,
        })
    }

    fn load(&self, id: Self::ObjectId) -> Result<Change, Self::LoadError> {
        let commit = Commit::read(self, id.into())?;
        let (author, resource) = parse_trailers(commit.trailers())?;
        let mut signatures = Signatures::try_from(&commit)?
            .into_iter()
            .collect::<Vec<_>>();
        let Some(signature) = signatures.pop() else {
            return Err(error::Load::ChangeNotSigned(id));
        };
        if !signatures.is_empty() {
            return Err(error::Load::TooManySignatures(id));
        }

        let tree = self.find_tree(commit.tree())?;
        let manifest = load_manifest(self, &tree)?;
        let contents = load_contents(self, &tree, &manifest)?;

        Ok(Change {
            id,
            revision: tree.id().into(),
            signature: signature.into(),
            author,
            resource,
            manifest,
            contents,
        })
    }
}

fn parse_trailers<'a>(
    mut trailers: impl Iterator<Item = &'a OwnedTrailer>,
) -> Result<(Option<Oid>, Oid), error::Load> {
    let (author, resource) = trailers.try_fold((None, None), |(author, resource), trailer| {
        match trailers::AuthorCommitTrailer::try_from(trailer) {
            Ok(trailer) => Ok((Some(trailer.oid().into()), resource)),
            Err(err) => match err {
                trailers::error::InvalidAuthorTrailer::NoTrailer
                | trailers::error::InvalidAuthorTrailer::NoValue => Ok((author, resource)),
                trailers::error::InvalidAuthorTrailer::WrongToken => {
                    let resource = trailers::ResourceCommitTrailer::try_from(trailer)?;
                    Ok((author, Some(resource.oid().into())))
                }
                err => Err(error::Load::from(err)),
            },
        }
    })?;
    let resource = resource
        .ok_or_else(|| error::Load::from(trailers::error::InvalidResourceTrailer::NoTrailer))?;
    Ok((author, resource))
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
    manifest: &store::Manifest,
) -> Result<entry::Contents, error::Load> {
    Ok(match manifest.history_type {
        HistoryType::Radicle | HistoryType::Automerge => {
            let contents_tree_entry = tree
                .get_name(CHANGE_BLOB_NAME)
                .ok_or_else(|| error::Load::NoChange(tree.id().into()))?;
            let contents_object = contents_tree_entry.to_object(repo)?;
            let contents_blob = contents_object
                .as_blob()
                .ok_or_else(|| error::Load::ChangeNotBlob(tree.id().into()))?;
            contents_blob.content().to_owned()
        }
    })
}

fn write_commit<O>(
    repo: &git2::Repository,
    author: Option<O>,
    resource: O,
    tips: Vec<O>,
    message: String,
    signature: Signature,
    tree: git2::Tree,
) -> Result<Oid, error::Create>
where
    O: AsRef<git2::Oid>,
{
    let author = author.map(|author| *author.as_ref());
    let resource = *resource.as_ref();

    let mut parents = tips.iter().map(|o| *o.as_ref()).collect::<Vec<_>>();
    parents.push(resource);
    parents.extend(author);

    let mut trailers: Vec<OwnedTrailer> =
        vec![trailers::ResourceCommitTrailer::from(resource).into()];
    trailers.extend(author.map(|author| trailers::AuthorCommitTrailer::from(author).into()));

    {
        let author = repo.signature()?;
        let mut headers = commit::Headers::new();
        headers.push(
            "gpgsig",
            &String::from_utf8(crypto::ssh::ExtendedSignature::from(signature).to_armored())?,
        );
        let author = commit::Author::try_from(&author)?;

        let commit = Commit::new(
            tree.id(),
            parents,
            author.clone(),
            author,
            headers,
            message,
            trailers,
        );
        commit
            .write(repo)
            .map(Oid::from)
            .map_err(error::Create::from)
    }
}

fn write_manifest(
    repo: &git2::Repository,
    manifest: &store::Manifest,
    contents: &entry::Contents,
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

    let change_blob = repo.blob(contents.as_ref())?;
    tb.insert(CHANGE_BLOB_NAME, change_blob, git2::FileMode::Blob.into())?;

    tb.write()
}
