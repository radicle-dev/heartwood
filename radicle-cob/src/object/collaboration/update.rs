// Copyright © 2022 The Radicle Link Contributors

use git_ext::Oid;
use nonempty::NonEmpty;

use crate::{
    change, change_graph::ChangeGraph, history::EntryId, CollaborativeObject, Embed, ObjectId,
    Store, TypeName,
};

use super::error;

/// Result of an `update` operation.
#[derive(Debug)]
pub struct Updated {
    /// The new head commit of the DAG.
    pub head: Oid,
    /// The newly updated collaborative object.
    pub object: CollaborativeObject,
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
pub fn update<S, I, G>(
    storage: &S,
    signer: &G,
    resource: Oid,
    parents: Vec<Oid>,
    identifier: &S::Identifier,
    args: Update,
) -> Result<Updated, error::Update>
where
    S: Store<I>,
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

    let mut object = ChangeGraph::load(storage, existing_refs.iter(), typename, &object_id)
        .map(|graph| graph.evaluate())
        .ok_or(error::Update::NoSuchObject)?;

    let change = storage.store(
        resource,
        parents,
        signer,
        change::Template {
            tips: object.tips().iter().cloned().collect(),
            embeds,
            contents: changes,
            type_name: typename.clone(),
            message,
        },
    )?;

    storage
        .update(identifier, typename, &object_id, &change)
        .map_err(|err| error::Update::Refs { err: Box::new(err) })?;

    let parents = change.parents.to_vec();
    let head = change.id;

    object.history.extend(change);

    Ok(Updated {
        object,
        head,
        parents,
    })
}
