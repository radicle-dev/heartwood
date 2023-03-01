// Copyright © 2022 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use git_ext::Oid;
use nonempty::NonEmpty;

use crate::{change, change_graph::ChangeGraph, CollaborativeObject, ObjectId, Store, TypeName};

use super::error;

/// The data required to update an object
pub struct Update {
    /// The type of history that will be used for this object.
    pub history_type: String,
    /// The CRDT changes to add to the object.
    pub changes: NonEmpty<Vec<u8>>,
    /// The object ID of the object to be updated.
    pub object_id: ObjectId,
    /// The typename of the object to be updated.
    pub typename: TypeName,
    /// The message to add when updating this object.
    pub message: String,
}

/// Update an existing [`CollaborativeObject`].
///
/// The `storage` is the backing storage for storing
/// [`crate::Change`]s at content-addressable locations. Please see
/// [`Store`] for further information.
///
/// The `signer` is expected to be a cryptographic signing key. This
/// ensures that the objects origin is cryptographically verifiable.
///
/// The `resource` is the parent of this object, for example a
/// software project. Its content-address is stored in the
/// object's history.
///
/// The `identifier` is a unqiue id that is passed through to the
/// [`crate::object::Storage`].
///
/// The `args` are the metadata for this [`CollaborativeObject`]
/// udpate. See [`Update`] for further information.
pub fn update<S, G>(
    storage: &S,
    signer: &G,
    resource: Oid,
    identifier: &S::Identifier,
    args: Update,
) -> Result<CollaborativeObject, error::Update>
where
    S: Store,
    G: crypto::Signer,
{
    let Update {
        ref typename,
        object_id,
        history_type,
        changes,
        message,
    } = args;

    let existing_refs = storage
        .objects(typename, &object_id)
        .map_err(|err| error::Update::Refs { err: Box::new(err) })?;

    let mut object = ChangeGraph::load(storage, existing_refs.iter(), typename, &object_id)
        .map(|graph| graph.evaluate())
        .ok_or(error::Update::NoSuchObject)?;

    let change = storage.store(
        resource,
        signer,
        change::Template {
            tips: object.tips().iter().cloned().collect(),
            history_type,
            contents: changes,
            typename: typename.clone(),
            message,
        },
    )?;

    storage
        .update(identifier, typename, &object_id, &change)
        .map_err(|err| error::Update::Refs { err: Box::new(err) })?;

    object.history.extend(
        change.id,
        change.signature.key,
        change.resource,
        change.contents,
        change.timestamp,
    );

    Ok(object)
}
