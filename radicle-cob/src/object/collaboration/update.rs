// Copyright © 2022 The Radicle Link Contributors
use std::iter;

use git_ext::Oid;
use nonempty::NonEmpty;
use radicle_crypto::PublicKey;

use crate::{
    change, change_graph::ChangeGraph, history::EntryId, CollaborativeObject, Embed, Evaluate,
    ObjectId, Store, TypeName,
};

use super::error;

/// Result of an `update` operation.
#[derive(Debug)]
pub struct Updated<T> {
    /// The new head commit of the DAG.
    pub head: Oid,
    /// The newly updated collaborative object.
    pub object: CollaborativeObject<T>,
    /// Entry parents.
    pub parents: Vec<EntryId>,
}

/// The data required to update an object
pub struct Update {
    /// The CRDT changes to add to the object.
    pub changes: NonEmpty<Vec<u8>>,
    /// The object ID of the object to be updated.
    pub object_id: ObjectId,
    /// The typename of the object to be updated.
    pub type_name: TypeName,
    /// The message to add when updating this object.
    pub message: String,
    /// Embedded files.
    pub embeds: Vec<Embed>,
}

/// Update an existing [`CollaborativeObject`].
///
/// The `storage` is the backing storage for storing
/// [`crate::Entry`]s at content-addressable locations. Please see
/// [`Store`] for further information.
///
/// The `signer` is expected to be a cryptographic signing key. This
/// ensures that the objects origin is cryptographically verifiable.
///
/// The `resource` is the resource this change lives under, eg. a project.
///
/// The `parents` are other the parents of this object, for example a
/// code commit.
///
/// The `identifier` is a unqiue id that is passed through to the
/// [`crate::object::Storage`].
///
/// The `args` are the metadata for this [`CollaborativeObject`]
/// udpate. See [`Update`] for further information.
pub fn update<T, S, G>(
    storage: &S,
    signer: &G,
    resource: Option<Oid>,
    related: Vec<Oid>,
    identifier: &PublicKey,
    args: Update,
) -> Result<Updated<T>, error::Update>
where
    T: Evaluate<S>,
    S: Store,
    G: crypto::Signer,
{
    let Update {
        type_name: ref typename,
        object_id,
        embeds,
        changes,
        message,
    } = args;

    let existing_refs = storage
        .objects(typename, &object_id)
        .map_err(|err| error::Update::Refs { err: Box::new(err) })?;

    let graph = ChangeGraph::load(storage, existing_refs.iter(), typename, &object_id)
        .ok_or(error::Update::NoSuchObject)?;
    let mut object: CollaborativeObject<T> =
        graph.evaluate(storage).map_err(error::Update::evaluate)?;

    // Create a commit for this change, but don't update any references yet.
    let entry = storage.store(
        resource,
        related,
        signer,
        change::Template {
            tips: object.history.tips().into_iter().collect(),
            embeds,
            contents: changes,
            type_name: typename.clone(),
            message,
        },
    )?;
    let head = entry.id;
    let parents = entry.parents.to_vec();

    // Try to apply this change to our object. This prevents storing invalid updates.
    // Note that if this returns with an error, we are left with an unreachable
    // commit object created above. This is fine, as it will eventually get
    // garbage-collected by Git.
    object
        .object
        .apply(&entry, iter::empty(), storage)
        .map_err(error::Update::evaluate)?;
    object.history.extend(entry);

    // Here we actually update the references to point to the new update.
    storage
        .update(identifier, typename, &object_id, &head)
        .map_err(|err| error::Update::Refs { err: Box::new(err) })?;

    Ok(Updated {
        object,
        head,
        parents,
    })
}
