// Copyright © 2022 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use nonempty::NonEmpty;

use crate::Store;

use super::*;

/// The metadata required for creating a new [`CollaborativeObject`].
pub struct Create {
    /// The type of history that will be used for this object.
    pub history_type: String,
    /// The CRDT history to initialize this object with.
    pub contents: NonEmpty<Vec<u8>>,
    /// The typename for this object.
    pub typename: TypeName,
    /// The message to add when creating this object.
    pub message: String,
}

impl Create {
    fn template(&self) -> change::Template<git_ext::Oid> {
        change::Template {
            typename: self.typename.clone(),
            history_type: self.history_type.clone(),
            tips: Vec::new(),
            message: self.message.clone(),
            contents: self.contents.clone(),
        }
    }
}

/// Create a new [`CollaborativeObject`].
///
/// The `storage` is the backing storage for storing
/// [`crate::Change`]s at content-addressable locations. Please see
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
pub fn create<S, G>(
    storage: &S,
    signer: &G,
    resource: Oid,
    identifier: &S::Identifier,
    args: Create,
) -> Result<CollaborativeObject, error::Create>
where
    S: Store,
    G: crypto::Signer,
{
    let Create { ref typename, .. } = &args;
    let init_change = storage
        .store(resource, signer, args.template())
        .map_err(error::Create::from)?;
    let object_id = init_change.id().into();

    storage
        .update(identifier, typename, &object_id, &init_change)
        .map_err(|err| error::Create::Refs { err: Box::new(err) })?;

    let history = History::new_from_root(
        *init_change.id(),
        init_change.signature.key,
        resource,
        init_change.contents,
        init_change.timestamp,
    );

    Ok(CollaborativeObject {
        manifest: Manifest {
            typename: args.typename,
            history_type: args.history_type,
        },
        history,
        id: object_id,
    })
}
