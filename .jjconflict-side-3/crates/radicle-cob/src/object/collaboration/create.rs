// Copyright © 2022 The Radicle Link Contributors

use nonempty::NonEmpty;

use crate::Embed;
use crate::Evaluate;

use super::*;

/// The metadata required for creating a new [`CollaborativeObject`].
pub struct Create {
    /// The CRDT history to initialize this object with.
    pub contents: NonEmpty<Vec<u8>>,
    /// The typename for this object.
    pub type_name: TypeName,
    /// The message to add when creating this object.
    pub message: String,
    /// Embedded content.
    pub embeds: Vec<Embed<Oid>>,
    /// COB version.
    pub version: Version,
}

impl Create {
    fn template(self) -> change::Template<oid::Oid> {
        change::Template {
            type_name: self.type_name,
            tips: Vec::new(),
            message: self.message,
            embeds: self.embeds,
            contents: self.contents,
        }
    }
}

/// Create a new [`CollaborativeObject`].
///
/// The `storage` is the backing storage for storing
/// [`crate::Entry`]s at content-addressable locations. Please see
/// [`crate::Store`] for further information.
///
/// The `signer` is expected to be a cryptographic signing key. This
/// ensures that the objects origin is cryptographically verifiable.
///
/// The `resource` is the parent of this object, for example a
/// software project. Its content-address is stored in the object's
/// history.
///
/// The `identifier` is a unique id that is passed through to the
/// [`crate::object::Storage`].
///
/// The `args` are the metadata for this [`CollaborativeObject`]. See
/// [`Create`] for further information.
pub fn create<T, S, G>(
    storage: &S,
    signer: &G,
    resource: Option<Oid>,
    related: Vec<Oid>,
    identifier: &<S as crate::object::Storage>::Namespace,
    args: Create,
) -> Result<CollaborativeObject<T>, error::Create>
where
    T: Evaluate<S>,
    S: crate::object::Storage
        + crate::change::Storage<
            ObjectId = crate::object::Oid,
            Parent = crate::object::Oid,
            Signatures = crate::ExtendedSignature,
        >,
    G: signature::Signer<crate::ExtendedSignature>,
{
    let type_name = args.type_name.clone();
    let version = args.version;
    let init_change = storage
        .store(resource, related, signer, args.template())
        .map_err(Into::<error::Create>::into)?;
    let object_id = init_change.id().into();
    let object = T::init(&init_change, storage).map_err(error::Create::evaluate)?;

    storage
        .update(identifier, &type_name, &object_id, &object_id)
        .map_err(|err| error::Create::Refs { err: Box::new(err) })?;

    let history = History::new_from_root(init_change);

    Ok(CollaborativeObject {
        manifest: Manifest::new(type_name, version),
        history,
        object,
        id: object_id,
    })
}
