pub mod doc;
pub mod issue;
pub mod label;
pub mod patch;
pub mod shared;
pub mod store;
pub mod transaction;
pub mod value;

pub use cob::{
    identity, object::collaboration::error, CollaborativeObject, Create, Entry, History, ObjectId,
    TypeName, Update,
};
use radicle_cob as cob;
use radicle_git_ext::Oid;

pub use radicle_cob::*;

use crate::{
    identity::{project::Identity, Did},
    node::NodeId,
    storage::git::Repository,
};

/// The `Author` of a [`create`] or [`update`].
///
/// **Note**: `Author` implements [`identity::Identity`], but since it
/// is not content-addressed, the [`identity::Identity::content_id`]
/// returns [`git2::Oid::zero`]. This means that if the `author` is
/// set in the updates the history entries for those changes will
/// contain the zero `Oid` for the `author` field.
pub struct Author {
    did: Did,
}

impl From<Did> for Author {
    fn from(did: Did) -> Self {
        Self { did }
    }
}

impl From<NodeId> for Author {
    fn from(node_id: NodeId) -> Self {
        Self {
            did: Did::from(node_id),
        }
    }
}

impl identity::Identity for Author {
    type Identifier = String;

    fn is_delegate(&self, delegation: &crypto::PublicKey) -> bool {
        *self.did == *delegation
    }

    fn content_id(&self) -> Oid {
        git2::Oid::zero().into()
    }
}

/// Create a new [`CollaborativeObject`].
///
/// The `repository` is the project this collaborative object is being
/// stored under.
///
/// The `signer` is used to cryptographically sign the changes made
/// for this update. **Note** that the public key for the signer must
/// match the key of the `Author` -- if it is set.
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
    args: Create<Author>,
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
/// for this update. **Note** that the public key for the signer must
/// match the key of the `Author` -- if it is set.
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
    args: Update<Author>,
) -> Result<CollaborativeObject, error::Update>
where
    G: crypto::Signer,
{
    let namespace = *signer.public_key();
    cob::update(repository, signer, project, &namespace, args)
}
