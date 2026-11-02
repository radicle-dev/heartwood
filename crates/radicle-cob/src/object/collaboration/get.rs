// Copyright © 2022 The Radicle Link Contributors

use crate::{
    CollaborativeObject, Evaluate, ObjectId, TypeName, change_graph::ChangeGraph, object::Objects,
};

use super::error;

/// Get a [`CollaborativeObject`], if it exists.
///
/// The `storage` is the backing storage for storing
/// [`crate::Entry`]s at content-addressable locations. Please see
/// [`crate::Store`] for further information.
///
/// The `typename` is the type of object to be found, while the
/// `object_id` is the identifier for the particular object under that
/// type.
pub fn get<T, S>(
    storage: &S,
    typename: &TypeName,
    oid: &ObjectId,
) -> Result<Option<CollaborativeObject<T>>, error::Retrieve>
where
    T: Evaluate<S>,
    S: crate::object::Storage,
    S: crate::change::Storage<
            ObjectId = crate::object::Oid,
            Parent = crate::object::Oid,
            PublicKey = crypto::PublicKey,
            Signature = crypto::Signature,
        >,
{
    let objects = storage
        .objects(typename, oid)
        .map_err(|err| error::Retrieve::Refs { err: Box::new(err) })?;

    get_from_objects(storage, typename, oid, &objects)
}

/// Get a [`CollaborativeObject`] from given `objects`.
///
/// The corresponding higher-level function is [`get`], which will resolve
/// `objects` from storage.
///
/// This function is useful for callers that want to avoid re-reading
/// references from storage.
pub fn get_from_objects<T, S>(
    storage: &S,
    typename: &TypeName,
    oid: &ObjectId,
    objects: &Objects,
) -> Result<Option<CollaborativeObject<T>>, error::Retrieve>
where
    T: Evaluate<S>,
    S: crate::change::Storage<
            ObjectId = crate::object::Oid,
            Parent = crate::object::Oid,
            PublicKey = crypto::PublicKey,
            Signature = crypto::Signature,
        >,
{
    ChangeGraph::load(storage, objects.iter(), typename, oid)
        .map(|graph| graph.evaluate(storage).map_err(error::Retrieve::evaluate))
        .transpose()
}
