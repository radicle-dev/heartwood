use git_ext::Oid;
use radicle_crypto::PublicKey;

use crate::{
    change_graph::ChangeGraph,
    history::EntryId,
    object::{Commit, Reference},
    CollaborativeObject, Evaluate, ObjectId, Store, TypeName,
};

use super::error;

/// Result of the [`merge`] operation.
pub struct Merged<T> {
    /// The new head commit of the DAG.
    pub head: Oid,
    /// The newly updated collaborative object.
    pub object: CollaborativeObject<T>,
    /// Entry parents.
    pub parents: Vec<EntryId>,
}

/// The date required to perform the [`merge`] operation.
pub struct Merge {
    /// The typename of the object to be merged.
    pub type_name: TypeName,
    /// The object ID of the object to be merged.
    pub object_id: ObjectId,
    /// The message to add when updating this object.
    pub message: String,
}

/// A newtype to ensure that the `Draft` store is distinguished from the
/// [`Published`] store when calling the [`merge`] function.
pub struct Draft<'a, S>(&'a S);

impl<'a, S> Draft<'a, S> {
    pub fn new(s: &'a S) -> Self {
        Self(s)
    }
}

impl<'a, S> AsRef<S> for Draft<'a, S> {
    fn as_ref(&self) -> &S {
        self.0
    }
}

/// A newtype to ensure that the `Published` store is distinguished from the
/// [`Draft`] store when calling the [`merge`] function.
pub struct Published<'a, S>(&'a S);

impl<'a, S> Published<'a, S> {
    pub fn new(s: &'a S) -> Self {
        Self(s)
    }
}

impl<'a, S> AsRef<S> for Published<'a, S> {
    fn as_ref(&self) -> &S {
        self.0
    }
}

/// Merge the changes made to a [`CollaborativeObject`], in the `draft` storage,
/// into the changes of the `published` storage.
///
/// The changes must come from the same `identifier` and so the reference
/// related to the `identifier` is updated with new head produced by the merge.
pub fn merge<T, D, P, G>(
    draft: &Draft<D>,
    published: &Published<P>,
    signer: &G,
    identifier: &PublicKey,
    args: Merge,
) -> Result<Merged<T>, error::Merge>
where
    T: Evaluate<D>,
    T: Evaluate<P>,
    D: Store,
    P: Store,
    G: crypto::Signer,
{
    let draft = draft.as_ref();
    let published = published.as_ref();
    let Merge {
        type_name,
        object_id,
        message,
    } = args;
    // Get the existing reference tips from both the draft and published
    // stores.
    let mut existing_refs = draft
        .objects(&type_name, &object_id)
        .map_err(|err| error::Merge::Refs { err: Box::new(err) })?;
    existing_refs.extend(
        published
            .objects(&type_name, &object_id)
            .map_err(|err| error::Merge::Refs { err: Box::new(err) })?,
    );

    // Create the merge entry in the published store and update the
    // `identifier`'s reference with the new tip.
    let merge = published.merge(
        existing_refs
            .iter()
            .map(
                |Reference {
                     target: Commit { id },
                     ..
                 }| *id,
            )
            .collect(),
        signer,
        type_name.clone(),
        message,
    )?;
    let head = merge.id;
    published
        .update(identifier, &type_name, &object_id, &head)
        .map_err(|err| error::Merge::Refs { err: Box::new(err) })?;
    draft
        .remove(identifier, &type_name, &object_id)
        .map_err(|err| error::Merge::Remove { err: Box::new(err) })?;

    let new_refs = published
        .objects(&type_name, &object_id)
        .map_err(|err| error::Merge::Refs { err: Box::new(err) })?;
    let graph = ChangeGraph::load(published, new_refs.iter(), &type_name, &object_id)
        .ok_or(error::Merge::NoSuchObject)?;
    let object: CollaborativeObject<T> = graph.evaluate(draft).map_err(error::Merge::evaluate)?;

    Ok(Merged {
        head,
        object,
        parents: merge.parents.to_vec(),
    })
}
