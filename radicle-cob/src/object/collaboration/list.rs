// Copyright © 2022 The Radicle Link Contributors

use crate::{change_graph::ChangeGraph, CollaborativeObject, Evaluate, Store, TypeName};

use super::error;

/// List a set of [`CollaborativeObject`].
///
/// The `storage` is the backing storage for storing
/// [`crate::Entry`]s at content-addressable locations. Please see
/// [`Store`] for further information.
///
/// The `typename` is the type of objects to be listed.
pub fn list<T, S>(
    storage: &S,
    typename: &TypeName,
) -> Result<Vec<CollaborativeObject<T>>, error::Retrieve>
where
    T: Evaluate<S>,
    S: Store,
{
    let references = storage
        .types(typename)
        .map_err(|err| error::Retrieve::Refs { err: Box::new(err) })?;
    log::trace!("loaded {} references", references.len());
    let mut result = Vec::new();
    for (oid, tip_refs) in references {
        log::trace!("loading object '{oid}'");
        let loaded = ChangeGraph::load(storage, tip_refs.iter(), typename, &oid)
            .map(|graph| graph.evaluate(storage).map_err(error::Retrieve::evaluate));

        match loaded {
            Some(Ok(obj)) => {
                log::trace!("object '{oid}' found");
                result.push(obj);
            }
            Some(Err(e)) => {
                log::trace!("object '{oid}' failed to load: {e}")
            }
            None => {
                log::trace!("object '{oid}' not found");
            }
        }
    }
    Ok(result)
}
