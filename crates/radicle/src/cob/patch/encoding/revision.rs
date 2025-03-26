use std::collections::{BTreeMap, BTreeSet};

use nonempty::NonEmpty;
use radicle_cob::EntryId;
use serde::Deserialize;

use crate::{
    cob::{
        thread::{Comment, CommentId, Edit, Reactions, Thread},
        ActorId, Author, DiffLocation, Timestamp,
    },
    git,
    patch::{self, encoding, RevisionId},
};

use super::Location;

/// A patch revision.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Revision {
    /// Revision identifier.
    id: RevisionId,
    /// Author of the revision.
    author: Author,
    /// Revision description.
    description: NonEmpty<Edit>,
    /// Base branch commit, used as a merge base.
    base: git::Oid,
    /// Reference to the Git object containing the code (revision head).
    oid: git::Oid,

    /// When this revision was created.
    timestamp: Timestamp,
    /// Review comments resolved by this revision.
    resolves: BTreeSet<(EntryId, CommentId)>,

    // V1 -> V2 conversion
    /// Discussion around this revision.
    discussion: Thread<Comment<Location>>,
    reviews: BTreeMap<ActorId, encoding::review::Review>,
    /// Reactions on code locations and revision itself
    #[serde(deserialize_with = "ser::deserialize_reactions")]
    reactions: BTreeMap<Option<Location>, Reactions>,
}

impl From<Revision> for patch::Revision {
    fn from(revision: Revision) -> Self {
        let Revision {
            id,
            author,
            description,
            base,
            oid,
            timestamp,
            resolves,
            discussion,
            reviews,
            reactions,
        } = revision;

        let discussion = decode_discussion(discussion, base);
        let reviews = decode_reviews(reviews, base);
        let reactions = decode_reactions(reactions, base);

        Self {
            id,
            author,
            description,
            base,
            oid,
            discussion,
            reviews,
            timestamp,
            resolves,
            reactions,
        }
    }
}

fn decode_reviews(
    reviews: BTreeMap<ActorId, encoding::review::Review>,
    context: git::Oid,
) -> BTreeMap<ActorId, patch::Review> {
    reviews
        .into_iter()
        .map(|(actor, review)| (actor, review.decode(context)))
        .collect()
}

fn decode_discussion(
    discussion: Thread<Comment<Location>>,
    context: git::Oid,
) -> Thread<Comment<DiffLocation>> {
    let comments = discussion
        .comments
        .into_iter()
        .map(|(id, c)| {
            let c = c.map(|c| c.map(|loc| loc.into_diff_location(context)));
            (id, c)
        })
        .collect();
    Thread {
        comments,
        timeline: discussion.timeline,
    }
}

fn decode_reactions(
    reactions: BTreeMap<Option<Location>, Reactions>,
    context: git::Oid,
) -> BTreeMap<Option<DiffLocation>, Reactions> {
    reactions
        .into_iter()
        .map(|(loc, rs)| {
            let loc = loc.map(|loc| loc.into_diff_location(context));
            (loc, rs)
        })
        .collect()
}

/// Helpers for de/serialization of patch data types.
mod ser {
    use std::collections::{BTreeMap, BTreeSet};

    use crate::{
        cob::{patch, thread::Reactions, ActorId},
        patch::encoding::Location,
    };

    /// Serialize a `Revision`'s reaction as an object containing the
    /// `location`, `emoji`, and all `authors` that have performed the
    /// same reaction.
    #[derive(Debug, serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Reaction {
        location: Option<Location>,
        emoji: patch::Reaction,
        authors: Vec<ActorId>,
    }

    impl Reaction {
        fn as_revision_reactions(
            reactions: Vec<Reaction>,
        ) -> BTreeMap<Option<Location>, Reactions> {
            reactions.into_iter().fold(
                BTreeMap::<Option<Location>, Reactions>::new(),
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

    /// Helper to deserialize a `Revision`'s reactions, the inverse of
    /// `serialize_reactions`.
    ///
    /// The `Vec` of [`Reaction`]s are deserialized and converted to a
    /// `BTreeMap<Option<CodeLocation>, Reactions>`.
    pub fn deserialize_reactions<'de, D>(
        deserializer: D,
    ) -> Result<BTreeMap<Option<Location>, Reactions>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ReactionsVisitor;

        impl<'de> serde::de::Visitor<'de> for ReactionsVisitor {
            type Value = Vec<Reaction>;

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

        let reactions = deserializer.deserialize_seq(ReactionsVisitor)?;
        Ok(Reaction::as_revision_reactions(reactions))
    }
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod test {
    use serde_json::json;

    use crate::{
        cob::{Author, DiffLocation, PartialLocation, Reaction},
        git, patch,
        prelude::Did,
    };

    use super::{Location, Revision};

    fn author() -> Author {
        Author::new(
            "did:key:z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx"
                .parse::<Did>()
                .unwrap(),
        )
    }

    fn base_oid() -> git::Oid {
        "e67a8f4d32c830c24ed68ea21707923480830511".parse().unwrap()
    }

    fn head_oid() -> git::Oid {
        "b455c819807cd7a7543d03215570c72b7cb452d7".parse().unwrap()
    }

    #[test]
    fn test_revision_deserialize_v1_location_migration() {
        let revision_json = json!({
            "id": "89d45fb371eb2622ba88188d474347cc526d80bb",
            "author": { "id": "did:key:z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx" },
            "description": [{
                "author": "z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx",
                "timestamp": 1710947885000_i64,
                "body": "Initial revision",
                "embeds": []
            }],
            "base": "e67a8f4d32c830c24ed68ea21707923480830511",
            "oid": "b455c819807cd7a7543d03215570c72b7cb452d7",
            "timestamp": 1710947885000_i64,
            "resolves": [],
            "discussion": {
                "comments": {
                    "2a47bc0c7dd6238ce3416e19d2ba57f3a7626f64": {
                        "id": "2a47bc0c7dd6238ce3416e19d2ba57f3a7626f64",
                        "author": "z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx",
                        "edits": [{
                            "author": "z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx",
                            "timestamp": 1710947885000_i64,
                            "body": "Great changes!",
                            "embeds": []
                        }],
                        "reactions": [],
                        "replyTo": null,
                        "resolved": false,
                        "location": {
                            "commit": "b455c819807cd7a7543d03215570c72b7cb452d7",
                            "path": "src/main.rs",
                            "old": {
                                "type": "lines",
                                "range": {
                                    "start": 10,
                                    "end": 12
                                }
                            },
                            "new": {
                                "type": "lines",
                                "range": {
                                    "start": 12,
                                    "end": 14
                                }
                            }
                        },
                        "embeds": []
                    }
                },
                "timeline": ["2a47bc0c7dd6238ce3416e19d2ba57f3a7626f64"]
            },
            "reviews": {},
            "reactions": [{
                "location": {
                    "commit": "b455c819807cd7a7543d03215570c72b7cb452d7",
                    "path": "src/utils.rs",
                    "old": {
                        "type": "lines",
                        "range": {
                            "start": 5,
                            "end": 7
                        }
                    },
                    "new": {
                        "type": "lines",
                        "range": {
                            "start": 8,
                            "end": 10
                        }
                    }
                },
                "emoji": "👍",
                "authors": ["z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx"]
            }]
        });

        let revision = serde_json::from_value::<Revision>(revision_json).unwrap();
        let decoded: patch::Revision = revision.into();

        // Check that the V1 PartialLocation was migrated to V2 DiffLocation
        let (comment_id, comment) = decoded.discussion().comments().next().unwrap();
        assert_eq!(
            comment_id.to_string(),
            "2a47bc0c7dd6238ce3416e19d2ba57f3a7626f64"
        );
        assert_eq!(comment.body(), "Great changes!");

        let location = comment.location().unwrap();
        assert_eq!(location.base, base_oid());
        assert_eq!(location.head, head_oid());
        assert_eq!(location.path.to_str().unwrap(), "src/main.rs");
        assert!(location.selection.is_none()); // V1 locations lose selection info

        // Check that reactions were also migrated
        let reactions = decoded.reactions();
        assert_eq!(reactions.len(), 1);
        let (location, reaction_set) = reactions.iter().next().unwrap();
        let location = location.as_ref().unwrap();
        assert_eq!(location.base, base_oid());
        assert_eq!(location.head, head_oid());
        assert_eq!(location.path.to_str().unwrap(), "src/utils.rs");
        assert!(location.selection.is_none()); // V1 locations lose selection info

        let reaction_emoji = Reaction::new('👍').unwrap();
        assert!(reaction_set.contains(&(*author().public_key(), reaction_emoji)));
    }

    #[test]
    fn test_revision_deserialize_v2_location_preserved() {
        let revision_json = json!({
            "id": "89d45fb371eb2622ba88188d474347cc526d80bb",
            "author": { "id": "did:key:z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx" },
            "description": [{
                "author": "z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx",
                "timestamp": 1710947885000_i64,
                "body": "Initial revision",
                "embeds": []
            }],
            "base": "e67a8f4d32c830c24ed68ea21707923480830511",
            "oid": "b455c819807cd7a7543d03215570c72b7cb452d7",
            "timestamp": 1710947885000_i64,
            "resolves": [],
            "discussion": {
                "comments": {
                    "2a47bc0c7dd6238ce3416e19d2ba57f3a7626f64": {
                        "id": "2a47bc0c7dd6238ce3416e19d2ba57f3a7626f64",
                        "author": "z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx",
                        "edits": [{
                            "author": "z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx",
                            "timestamp": 1710947885000_i64,
                            "body": "Great changes!",
                            "embeds": []
                        }],
                        "reactions": [],
                        "replyTo": null,
                        "resolved": false,
                        "location": {
                            "base": "e67a8f4d32c830c24ed68ea21707923480830511",
                            "head": "b455c819807cd7a7543d03215570c72b7cb452d7",
                            "path": "src/main.rs",
                            "selection": {
                                "hunk": 0,
                                "range": {
                                    "type": "lines",
                                    "range": {
                                        "start": 2,
                                        "end": 4
                                    }
                                }
                            }
                        },
                        "embeds": []
                    }
                },
                "timeline": ["2a47bc0c7dd6238ce3416e19d2ba57f3a7626f64"]
            },
            "reviews": {},
            "reactions": [{
                "location": {
                    "base": "e67a8f4d32c830c24ed68ea21707923480830511",
                    "head": "b455c819807cd7a7543d03215570c72b7cb452d7",
                    "path": "src/utils.rs",
                    "selection": {
                        "hunk": 1,
                        "range": {
                            "type": "lines",
                            "range": {
                                "start": 0,
                                "end": 3
                            }
                        }
                    }
                },
                "emoji": "👍",
                "authors": ["z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx"]
            }]
        });

        let revision = serde_json::from_value::<Revision>(revision_json).unwrap();
        let decoded: patch::Revision = revision.into();

        // Check that the V2 DiffLocation was preserved
        let (comment_id, comment) = decoded.discussion().comments().next().unwrap();
        assert_eq!(
            comment_id.to_string(),
            "2a47bc0c7dd6238ce3416e19d2ba57f3a7626f64"
        );
        assert_eq!(comment.body(), "Great changes!");

        let location = comment.location().unwrap();
        assert_eq!(location.base, base_oid());
        assert_eq!(location.head, head_oid());
        assert_eq!(location.path.to_str().unwrap(), "src/main.rs");

        // V2 locations preserve selection info
        let selection = location.selection.as_ref().unwrap();
        assert_eq!(selection.index(), 0);
        assert_eq!(selection.range().line_start(), 2);
        assert_eq!(selection.range().line_end(), 4);

        // Check that reactions were also preserved
        let reactions = decoded.reactions();
        assert_eq!(reactions.len(), 1);
        let (location, reaction_set) = reactions.iter().next().unwrap();
        let location = location.as_ref().unwrap();
        assert_eq!(location.base, base_oid());
        assert_eq!(location.head, head_oid());
        assert_eq!(location.path.to_str().unwrap(), "src/utils.rs");

        // V2 locations preserve selection info
        let selection = location.selection.as_ref().unwrap();
        assert_eq!(selection.index(), 1);
        assert_eq!(selection.range().line_start(), 0);
        assert_eq!(selection.range().line_end(), 3);

        let reaction_emoji = Reaction::new('👍').unwrap();
        assert!(reaction_set.contains(&(*author().public_key(), reaction_emoji)));
    }

    #[test]
    fn test_revision_deserialize_mixed_locations() {
        // Test a revision that has both V1 and V2 locations mixed together
        let revision_json = json!({
            "id": "89d45fb371eb2622ba88188d474347cc526d80bb",
            "author": { "id": "did:key:z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx" },
            "description": [{
                "author": "z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx",
                "timestamp": 1710947885000_i64,
                "body": "Initial revision",
                "embeds": []
            }],
            "base": "e67a8f4d32c830c24ed68ea21707923480830511",
            "oid": "b455c819807cd7a7543d03215570c72b7cb452d7",
            "timestamp": 1710947885000_i64,
            "resolves": [],
            "discussion": {
                "comments": {
                    "2a47bc0c7dd6238ce3416e19d2ba57f3a7626f64": {
                        "id": "2a47bc0c7dd6238ce3416e19d2ba57f3a7626f64",
                        "author": "z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx",
                        "edits": [{
                            "author": "z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx",
                            "timestamp": 1710947885000_i64,
                            "body": "V1 location comment",
                            "embeds": []
                        }],
                        "timestamp": 1710947885000_i64,
                        "replyTo": null,
                        "resolved": false,
                        "location": {
                            "commit": "b455c819807cd7a7543d03215570c72b7cb452d7",
                            "path": "src/old.rs",
                            "old": {
                                "type": "lines",
                                "range": { "start": 1, "end": 3 }
                            },
                            "new": {
                                "type": "lines",
                                "range": { "start": 1, "end": 3 }
                            }
                        },
                        "reactions": [],
                        "embeds": []
                    },
                    "89d45fb371eb2622ba88188d474347cc526d80bb": {
                        "id": "89d45fb371eb2622ba88188d474347cc526d80bb",
                        "author": "z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx",
                        "edits": [{
                            "author": "z6MkwPUeUS2fJMfc2HZN1RQTQcTTuhw4HhPySB8JeUg2mVvx",
                            "timestamp": 1710947885000_i64,
                            "body": "V2 location comment",
                            "embeds": []
                        }],
                        "timestamp": 1710947885000_i64,
                        "replyTo": null,
                        "resolved": false,
                        "location": {
                            "base": "e67a8f4d32c830c24ed68ea21707923480830511",
                            "head": "b455c819807cd7a7543d03215570c72b7cb452d7",
                            "path": "src/new.rs",
                            "selection": {
                                "hunk": 2,
                                "range": {
                                    "type": "lines",
                                    "range": { "start": 5, "end": 8 }
                                }
                            }
                        },
                        "reactions": [],
                        "embeds": []
                    }
                },
                "timeline": ["2a47bc0c7dd6238ce3416e19d2ba57f3a7626f64", "89d45fb371eb2622ba88188d474347cc526d80bb"]
            },
            "reviews": {},
            "reactions": []
        });

        let revision = serde_json::from_value::<Revision>(revision_json).unwrap();
        let decoded: patch::Revision = revision.into();

        let comments: Vec<_> = decoded.discussion().comments().collect();
        assert_eq!(comments.len(), 2);

        // Check V1 location was migrated
        let (comment1_id, comment1) = &comments[0];
        assert_eq!(
            comment1_id.to_string(),
            "2a47bc0c7dd6238ce3416e19d2ba57f3a7626f64"
        );
        assert_eq!(comment1.body(), "V1 location comment");
        let location1 = comment1.location().unwrap();
        assert_eq!(location1.base, base_oid()); // Context provided
        assert_eq!(location1.head, head_oid());
        assert_eq!(location1.path.to_str().unwrap(), "src/old.rs");
        assert!(location1.selection.is_none()); // V1 loses selection

        // Check V2 location was preserved
        let (comment2_id, comment2) = &comments[1];
        assert_eq!(
            comment2_id.to_string(),
            "89d45fb371eb2622ba88188d474347cc526d80bb"
        );
        assert_eq!(comment2.body(), "V2 location comment");
        let location2 = comment2.location().unwrap();
        assert_eq!(location2.base, base_oid());
        assert_eq!(location2.head, head_oid());
        assert_eq!(location2.path.to_str().unwrap(), "src/new.rs");

        // V2 preserves selection
        let selection2 = location2.selection.as_ref().unwrap();
        assert_eq!(selection2.index(), 2);
        assert_eq!(selection2.range().line_start(), 5);
        assert_eq!(selection2.range().line_end(), 8);
    }

    #[test]
    fn test_location_v1_to_v2_migration() {
        let base_context = base_oid();
        let partial_location = PartialLocation {
            commit: head_oid(),
            path: "src/test.rs".into(),
            old: Some(crate::cob::common::CodeRange::lines(10..15)),
            new: Some(crate::cob::common::CodeRange::lines(12..17)),
        };

        let v1_location = Location::V1(partial_location);
        let diff_location = v1_location.into_diff_location(base_context);

        assert_eq!(diff_location.base, base_context);
        assert_eq!(diff_location.head, head_oid());
        assert_eq!(diff_location.path.to_str().unwrap(), "src/test.rs");
        assert!(diff_location.selection.is_none()); // V1 migration loses selection
    }

    #[test]
    fn test_location_v2_preserved() {
        let base_context = base_oid();
        let diff_location = DiffLocation {
            base: base_oid(),
            head: head_oid(),
            path: "src/test.rs".into(),
            selection: Some(crate::cob::common::HunkIndex::new(
                1,
                crate::cob::common::CodeRange::lines(5..10),
            )),
        };

        let v2_location = Location::V2(diff_location.clone());
        let preserved_location = v2_location.into_diff_location(base_context);

        assert_eq!(preserved_location, diff_location); // Should be identical
    }
}
