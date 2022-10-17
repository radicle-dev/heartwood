// Copyright © 2022 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use crate::{change_graph::ChangeGraph, identity, CollaborativeObject, ObjectId, Store, TypeName};

use super::error;

/// Get a [`CollaborativeObject`], if it exists.
///
/// The `storage` is the backing storage for storing
/// [`crate::Change`]s at content-addressable locations. Please see
/// [`Store`] for further information.
///
/// The `resource` is the parent of this object, for example a
/// software project.
///
/// The `typename` is the type of object to be found, while the `oid`
/// is the identifier for the particular object under that type.
pub fn get<S, Resource>(
    storage: &S,
    resource: &Resource,
    typename: &TypeName,
    oid: &ObjectId,
) -> Result<Option<CollaborativeObject>, error::Retrieve>
where
    S: Store<Resource>,
    Resource: identity::Identity,
{
    let tip_refs = storage
        .objects(&resource.identifier(), typename, oid)
        .map_err(|err| error::Retrieve::Refs { err: Box::new(err) })?;
    Ok(ChangeGraph::load(storage, tip_refs.iter(), typename, oid).map(|graph| graph.evaluate()))
}
