// Copyright © 2022 The Radicle Link Contributors
//
// This file is part of radicle-link, distributed under the GPLv3 with Radicle
// Linking Exception. For full terms see the included LICENSE file.

use crate::Store;

use super::*;

/// The metadata required for creating a new [`CollaborativeObject`].
pub struct Create<Author> {
    /// The identity of the author for this object's first change.
    pub author: Option<Author>,
    /// The CRDT history to initialize this object with.
    pub contents: Contents,
    /// The typename for this object.
    pub typename: TypeName,
    /// The message to add when creating this object.
    pub message: String,
}

impl<Author> Create<Author> {
    fn create_spec(&self) -> change::Create<git_ext::Oid> {
        change::Create {
            typename: self.typename.clone(),
            tips: Vec::new(),
            message: self.message.clone(),
            contents: self.contents.clone(),
        }
    }
}

/// Create a new [`CollaborativeObject`].
///
/// The `storage` is the backing storage for storing
/// [`crate::Change`]s at content-addressable locations. Please see
/// [`Store`] for further information.
///
/// The `signer` is expected to be a cryptographic signing key. This
/// ensures that the objects origin is cryptographically verifiable.
///
/// The `resource` is the parent of this object, for example a
/// software project. Its content-address is stored in the object's
/// history.
///
/// The `identifier` is a unqiue id that is passed through to the
/// [`crate::object::Storage`].
///
/// The `args` are the metadata for this [`CollaborativeObject`]. See
/// [`Create`] for further information.
pub fn create<S, Signer, Author, Resource>(
    storage: &S,
    signer: Signer,
    resource: &Resource,
    identifier: &S::Identifier,
    args: Create<Author>,
) -> Result<CollaborativeObject, error::Create>
where
    S: Store,
    Author: Identity,
    Author::Identifier: Clone + PartialEq,
    Resource: Identity,
    Signer: crypto::Signer,
{
    let Create {
        author,
        ref contents,
        ref typename,
        ..
    } = &args;

    let content = match author {
        None => None,
        Some(author) => {
            if !author.is_delegate(signer.public_key()) {
                return Err(error::Create::SignerIsNotAuthor);
            } else {
                Some(author.content_id())
            }
        }
    };

    let init_change = storage
        .create(content, resource.content_id(), &signer, args.create_spec())
        .map_err(error::Create::from)?;

    let history = History::new_from_root(
        *init_change.id(),
        content,
        resource.content_id(),
        contents.clone(),
    );

    let object_id = init_change.id().into();
    storage
        .update(identifier, typename, &object_id, &init_change)
        .map_err(|err| error::Create::Refs { err: Box::new(err) })?;

    Ok(CollaborativeObject {
        typename: args.typename,
        history,
        id: init_change.id().into(),
    })
}
