// Copyright © 2022 The Radicle Link Contributors

use crate::{change_graph::ChangeGraph, CollaborativeObject, Evaluate, ObjectId, Store, TypeName};

use super::error;

/// Get a [`CollaborativeObject`], if it exists.
///
/// The `storage` is the backing storage for storing
/// [`crate::Entry`]s at content-addressable locations. Please see
/// [`Store`] for further information.
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
    S: Store,
{
    let tip_refs = storage
        .objects(typename, oid)
        .map_err(|err| error::Retrieve::Refs { err: Box::new(err) })?;

    ChangeGraph::load(storage, tip_refs.iter(), typename, oid)
        .map(|graph| graph.evaluate(storage).map_err(error::Retrieve::evaluate))
        .transpose()
}
