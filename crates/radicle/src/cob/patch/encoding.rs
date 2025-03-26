//! Home for separating serialization and deserialization of [`Patch`] data from
//! the final [`Patch`] state.
//!
//! [`Patch`]: super::Patch

pub mod review;
pub mod revision;

use crate::cob::{DiffLocation, PartialLocation};
use crate::git;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub(in crate::cob::patch) enum Location {
    V2(DiffLocation),
    V1(PartialLocation),
}

impl Location {
    pub fn into_diff_location(self, context: git::Oid) -> DiffLocation {
        match self {
            Location::V2(loc) => loc,
            Location::V1(PartialLocation { commit, path, .. }) => DiffLocation {
                base: context,
                head: commit,
                path,
                // N.b. We would need to figure out where the lines translate it
                // into the hunk – which would require using a `Repository` to
                // look up.
                // Instead, we just set the selection to `None` and preserve the
                // rest of the information.
                selection: None,
            },
        }
    }
}

/// Helpers for de/serialization of patch data types.
pub(super) mod ser {
    use std::collections::BTreeMap;

    use serde::{ser::SerializeSeq as _, Deserialize, Serialize};

    use crate::cob::{patch, thread::Reactions, ActorId};

    #[cfg(test)]
    use std::{collections::BTreeSet, marker::PhantomData};

    /// Serialize a `Revision`'s reaction as an object containing the
    /// `location`, `emoji`, and all `authors` that have performed the
    /// same reaction.
    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Reaction<T> {
        location: Option<T>,
        emoji: patch::Reaction,
        authors: Vec<ActorId>,
    }

    impl<T> Reaction<T> {
        #[cfg(test)]
        fn as_revision_reactions(reactions: Vec<Self>) -> BTreeMap<Option<T>, Reactions>
        where
            T: Ord,
        {
            reactions.into_iter().fold(
                BTreeMap::<Option<T>, Reactions>::new(),
                |mut reactions,
                 Reaction {
                     location,
                     emoji,
                     authors,
                 }| {
                    let mut inner = authors
                        .into_iter()
                        .map(|author| (author, emoji))
                        .collect::<BTreeSet<_>>();
                    let entry = reactions.entry(location).or_default();
                    entry.append(&mut inner);
                    reactions
                },
            )
        }
    }

    /// Helper to serialize a `Revision`'s reactions, since
    /// `CodeLocation` cannot be a key for a JSON object.
    ///
    /// The set `reactions` are first turned into a set of
    /// [`Reaction`]s and then serialized via a `Vec`.
    pub fn serialize_reactions<T, S>(
        reactions: &BTreeMap<Option<T>, Reactions>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
        T: Clone + Serialize,
    {
        let reactions = reactions
            .iter()
            .flat_map(|(location, reaction)| {
                let reactions = reaction.iter().fold(
                    BTreeMap::new(),
                    |mut acc: BTreeMap<&patch::Reaction, Vec<_>>, (author, emoji)| {
                        acc.entry(emoji).or_default().push(*author);
                        acc
                    },
                );
                reactions
                    .into_iter()
                    .map(|(emoji, authors)| Reaction {
                        location: location.clone(),
                        emoji: *emoji,
                        authors,
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let mut s = serializer.serialize_seq(Some(reactions.len()))?;
        for r in &reactions {
            s.serialize_element(r)?;
        }
        s.end()
    }

    /// Helper to deserialize a `Revision`'s reactions, the inverse of
    /// `serialize_reactions`.
    ///
    /// The `Vec` of [`Reaction`]s are deserialized and converted to a
    /// `BTreeMap<Option<CodeLocation>, Reactions>`.
    #[cfg(test)]
    pub fn deserialize_reactions<'de, T, D>(
        deserializer: D,
    ) -> Result<BTreeMap<Option<T>, Reactions>, D::Error>
    where
        D: serde::Deserializer<'de>,
        T: Ord + Deserialize<'de>,
    {
        struct ReactionsVisitor<T>(PhantomData<T>);

        impl<'de, T> serde::de::Visitor<'de> for ReactionsVisitor<T>
        where
            T: Deserialize<'de>,
        {
            type Value = Vec<Reaction<T>>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a reaction of the form {'location', 'emoji', 'authors'}")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut reactions = Vec::new();
                while let Some(reaction) = seq.next_element()? {
                    reactions.push(reaction);
                }
                Ok(reactions)
            }
        }

        let reactions = deserializer.deserialize_seq(ReactionsVisitor::<T>(PhantomData))?;
        Ok(Reaction::as_revision_reactions(reactions))
    }
}
