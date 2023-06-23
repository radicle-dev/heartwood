// Copyright © 2022 The Radicle Link Contributors

use crate::{change_graph::ChangeGraph, CollaborativeObject, Store, TypeName};

use super::error;

/// List a set of [`CollaborativeObject`].
///
/// The `storage` is the backing storage for storing
/// [`crate::Change`]s at content-addressable locations. Please see
/// [`Store`] for further information.
///
/// The `typename` is the type of objects to be listed.
pub fn list<S, I>(
    storage: &S,
    typename: &TypeName,
) -> Result<Vec<CollaborativeObject>, error::Retrieve>
where
    S: Store<I>,
{
    let references = storage
        .types(typename)
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
