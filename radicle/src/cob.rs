pub mod common;
pub mod issue;
pub mod patch;
pub mod store;
pub mod thread;

pub use radicle_crdt::clock::Physical as Timestamp;

pub use cob::{
    identity, object::collaboration::error, CollaborativeObject, Contents, Create, Entry, History,
    ObjectId, TypeName, Update,
};
pub use common::*;

use radicle_cob as cob;
use radicle_git_ext::Oid;

use crate::{identity::project::Identity, storage::git::Repository};

/// Create a new [`CollaborativeObject`].
///
/// The `repository` is the project this collaborative object is being
/// stored under.
///
/// The `signer` is used to cryptographically sign the changes made
/// for this update.
///
/// The `project` is used to store its content-address in the history
/// of changes for the collaborative object.
///
/// The `args` are the metadata for this [`CollaborativeObject`]
/// udpate. See [`Update`] for further information.
pub fn create<G>(
    repository: &Repository,
    signer: &G,
    project: &Identity<Oid>,
    args: Create,
) -> Result<CollaborativeObject, error::Create>
where
    G: crypto::Signer,
{
    let namespace = *signer.public_key();
    cob::create(repository, signer, project, &namespace, args)
}

/// Get a [`CollaborativeObject`], if it exists.
///
/// The `repository` is the project this collaborative object is being
/// stored under.
///
/// The `typename` is the type of object to be found, while the
/// `object_id` is the identifier for the particular object under that
/// type.
pub fn get(
    repository: &Repository,
    typename: &TypeName,
    object_id: &ObjectId,
) -> Result<Option<CollaborativeObject>, error::Retrieve> {
    cob::get(repository, typename, object_id)
}

/// List a set of [`CollaborativeObject`].
///
/// The `repository` is the project this collaborative object is being
/// stored under.
///
/// The `typename` is the type of objects to listed.
pub fn list(
    repository: &Repository,
    typename: &TypeName,
) -> Result<Vec<CollaborativeObject>, error::Retrieve> {
    cob::list(repository, typename)
}

/// Update an existing [`CollaborativeObject`].
///
/// The `repository` is the project this collaborative object is being
/// stored under.
///
/// The `signer` is used to cryptographically sign the changes made
/// for this update.
///
/// The `project` is used to store its content-address in the history
/// of changes for the collaborative object.
///
/// The `args` are the metadata for this [`CollaborativeObject`]
/// udpate. See [`Update`] for further information.
pub fn update<G>(
    repository: &Repository,
    signer: &G,
    project: &Identity<Oid>,
    args: Update,
) -> Result<CollaborativeObject, error::Update>
where
    G: crypto::Signer,
{
    let namespace = *signer.public_key();
    cob::update(repository, signer, project, &namespace, args)
}
