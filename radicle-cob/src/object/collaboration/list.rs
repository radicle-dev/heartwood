// Copyright © 2022 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use crate::{change_graph::ChangeGraph, CollaborativeObject, Store, TypeName};

use super::error;

/// List a set of [`CollaborativeObject`].
///
/// The `storage` is the backing storage for storing
/// [`crate::Change`]s at content-addressable locations. Please see
/// [`Store`] for further information.
///
/// The `identifier` is a unqiue id that is passed through to the
/// [`crate::object::Storage`].
///
/// The `typename` is the type of objects to listed.
pub fn list<S>(
    storage: &S,
    identifier: &S::Identifier,
    typename: &TypeName,
) -> Result<Vec<CollaborativeObject>, error::Retrieve>
where
    S: Store,
{
    let references = storage
        .types(identifier, typename)
        .map_err(|err| error::Retrieve::Refs { err: Box::new(err) })?;
    log::trace!("loaded {} references", references.len());
    let mut result = Vec::new();
    for (oid, tip_refs) in references {
        log::trace!("loading object '{}'", oid);
        let loaded = ChangeGraph::load(storage, tip_refs.iter(), typename, &oid)
            .map(|graph| graph.evaluate());

        match loaded {
            Some(obj) => {
                log::trace!("object '{}' found", oid);
                result.push(obj);
            }
            None => {
                log::trace!("object '{}' not found", oid);
            }
        }
    }
    Ok(result)
}
