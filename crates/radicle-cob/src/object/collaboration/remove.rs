// Copyright © 2022 The Radicle Link Contributors

use crate::{ObjectId, Store, TypeName};

use super::error;

/// Remove a [`crate::CollaborativeObject`].
///
/// The `storage` is the backing storage for storing
/// [`crate::Entry`]s at content-addressable locations. Please see
/// [`Store`] for further information.
///
/// The `typename` is the type of object to be found, while the
/// `object_id` is the identifier for the particular object under that
/// type.
pub fn remove<S>(
    storage: &S,
    identifier: &S::Namespace,
    typename: &TypeName,
    oid: &ObjectId,
) -> Result<(), error::Remove>
where
    S: Store,
{
    storage
        .remove(identifier, typename, oid)
        .map_err(|e| error::Remove { err: e.into() })?;

    Ok(())
}
