// Copyright © 2022 The Radicle Link Contributors

use nonempty::NonEmpty;

use crate::Embed;
use crate::Store;

use super::*;

/// The metadata required for creating a new [`CollaborativeObject`].
pub struct Create {
    /// The CRDT history to initialize this object with.
    pub contents: NonEmpty<Vec<u8>>,
    /// The typename for this object.
    pub type_name: TypeName,
    /// The message to add when creating this object.
    pub message: String,
    /// Embedded content.
    pub embeds: Vec<Embed>,
    /// COB version.
    pub version: Version,
}

impl Create {
    fn template(self) -> change::Template<git_ext::Oid> {
        change::Template {
            type_name: self.type_name,
            tips: Vec::new(),
            message: self.message,
            embeds: self.embeds,
            contents: self.contents,
        }
    }
}

/// Create a new [`CollaborativeObject`].
///
/// The `storage` is the backing storage for storing
/// [`crate::Entry`]s at content-addressable locations. Please see
/// [`Store`] for further information.
///
/// The `signer` is expected to be a cryptographic signing key. This
/// ensures that the objects origin is cryptographically verifiable.
///
/// The `resource` is the parent of this object, for example a
/// software project. Its content-address is stored in the object's
/// history.
///
/// The `identifier` is a unqiue id that is passed through to the
/// [`crate::object::Storage`].
///
/// The `args` are the metadata for this [`CollaborativeObject`]. See
/// [`Create`] for further information.
pub fn create<S, I, G>(
    storage: &S,
    signer: &G,
    resource: Oid,
    parents: Vec<Oid>,
    identifier: &S::Identifier,
    args: Create,
) -> Result<CollaborativeObject, error::Create>
where
    S: Store<I>,
    G: crypto::Signer,
{
    let type_name = args.type_name.clone();
    let version = args.version;
    let init_change = storage
        .store(resource, parents, signer, args.template())
        .map_err(error::Create::from)?;
    let object_id = init_change.id().into();

    storage
        .update(identifier, &type_name, &object_id, &init_change.id)
        .map_err(|err| error::Create::Refs { err: Box::new(err) })?;

    let history = History::new_from_root(init_change);

    Ok(CollaborativeObject {
        manifest: Manifest::new(type_name, version),
        history,
        id: object_id,
    })
}
