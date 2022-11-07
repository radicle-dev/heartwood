// Copyright © 2022 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use crate::{
    change, change_graph::ChangeGraph, identity::Identity, CollaborativeObject, Contents, ObjectId,
    Store, TypeName,
};

use super::error;

/// The data required to update an object
pub struct Update<Author> {
    /// The identity of the author for the update of this object.
    pub author: Option<Author>,
    /// The CRDT changes to add to the object.
    pub changes: Contents,
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
pub fn update<S, Signer, Resource, Author>(
    storage: &S,
    signer: Signer,
    resource: &Resource,
    identifier: &S::Identifier,
    args: Update<Author>,
) -> Result<CollaborativeObject, error::Update>
where
    S: Store,
    Author: Identity,
    Author::Identifier: Clone + PartialEq,
    Resource: Identity,
    Signer: crypto::Signer,
{
    let Update {
        author,
        ref typename,
        object_id,
        changes,
        message,
    } = args;

    let content = match author {
        None => None,
        Some(author) => {
            if !author.is_delegate(signer.public_key()) {
                return Err(error::Update::SignerIsNotAuthor);
            } else {
                Some(author.content_id())
            }
        }
    };

    let existing_refs = storage
        .objects(identifier, typename, &object_id)
        .map_err(|err| error::Update::Refs { err: Box::new(err) })?;

    let mut object = ChangeGraph::load(storage, existing_refs.iter(), typename, &object_id)
        .map(|graph| graph.evaluate())
        .ok_or(error::Update::NoSuchObject)?;

    let change = storage.create(
        content,
        resource.content_id(),
        &signer,
        change::Create {
            tips: object.tips().iter().cloned().collect(),
            contents: changes.clone(),
            typename: typename.clone(),
            message,
        },
    )?;

    object
        .history
        .extend(change.id, content, change.resource, changes);
    storage
        .update(identifier, typename, &object_id, &change)
        .map_err(|err| error::Update::Refs { err: Box::new(err) })?;

    Ok(object)
}
