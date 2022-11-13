#![allow(clippy::large_enum_variant)]
use std::convert::TryFrom;
use std::ops::ControlFlow;
use std::str::FromStr;

use automerge::{Automerge, ObjType};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::cob::doc::Document;
use crate::cob::shared::*;
use crate::cob::store::{Error, FromHistory, Store};
use crate::cob::transaction::TransactionError;
use crate::cob::{Contents, History, ObjectId, TypeName};
use crate::prelude::*;

pub static TYPENAME: Lazy<TypeName> =
    Lazy::new(|| FromStr::from_str("xyz.radicle.label").expect("type name is valid"));

/// Identifier for a label.
pub type LabelId = ObjectId;

/// Describes a label.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct Label {
    pub name: String,
    pub description: String,
    pub color: Color,
}

impl FromHistory for Label {
    fn type_name() -> &'static TypeName {
        &TYPENAME
    }

    fn from_history(history: &History) -> Result<Self, Error> {
        Label::try_from(history)
    }
}

impl TryFrom<&History> for Label {
    type Error = Error;

    fn try_from(history: &History) -> Result<Self, Self::Error> {
        let doc = history.traverse(Automerge::new(), |mut doc, entry| {
            match entry.contents() {
                Contents::Automerge(bytes) => {
                    match automerge::Change::from_bytes(bytes.clone()) {
                        Ok(change) => {
                            doc.apply_changes([change]).ok();
                        }
                        Err(_err) => {
                            // Ignore
                        }
                    }
                }
            }
            ControlFlow::Continue(doc)
        });
        let label = Label::try_from(doc)?;

        Ok(label)
    }
}

impl TryFrom<Automerge> for Label {
    type Error = Error;

    fn try_from(doc: Automerge) -> Result<Self, Self::Error> {
        let doc = Document::new(&doc);
        let obj_id = doc.get_id(automerge::ObjId::Root, "label")?;
        let name = doc.get(&obj_id, "name")?;
        let description = doc.get(&obj_id, "description")?;
        let color = doc.get(&obj_id, "color")?;

        Ok(Self {
            name,
            description,
            color,
        })
    }
}

pub struct LabelStore<'a> {
    store: Store<'a, Label>,
}

impl<'a> LabelStore<'a> {
    pub fn new(store: Store<'a, Label>) -> Self {
        Self { store }
    }

    pub fn create<G: Signer>(
        &self,
        name: &str,
        description: &str,
        color: &Color,
        signer: &G,
    ) -> Result<LabelId, Error> {
        let author = self.store.author();
        let _timestamp = Timestamp::now();
        let contents = events::create(&author, name, description, color)?;
        let cob = self.store.create("Create label", contents, signer)?;

        Ok(*cob.id())
    }

    pub fn get(&self, id: &LabelId) -> Result<Option<Label>, Error> {
        self.store.get(id)
    }
}

mod events {
    use super::*;
    use automerge::{
        transaction::{CommitOptions, Transactable},
        ObjId,
    };

    pub fn create(
        _author: &Author,
        name: &str,
        description: &str,
        color: &Color,
    ) -> Result<Contents, TransactionError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(TransactionError::InvalidValue("name"));
        }
        let mut doc = Automerge::new();

        doc.transact_with::<_, _, TransactionError, _, ()>(
            |_| CommitOptions::default().with_message("Create label".to_owned()),
            |tx| {
                let label = tx.put_object(ObjId::Root, "label", ObjType::Map)?;

                tx.put(&label, "name", name)?;
                tx.put(&label, "description", description)?;
                tx.put(&label, "color", color.to_string())?;

                Ok(label)
            },
        )
        .map_err(|failure| failure.error)?;

        Ok(Contents::Automerge(doc.save_incremental()))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::test;

    #[test]
    fn test_label_create_and_get() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, signer, project) = test::setup::context(&tmp);
        let store = Store::open(*signer.public_key(), &project).unwrap();
        let labels = store.labels();
        let label_id = labels
            .create(
                "bug",
                "Something that doesn't work",
                &Color::from_str("#ff0000").unwrap(),
                &signer,
            )
            .unwrap();
        let label = labels.get(&label_id).unwrap().unwrap();

        assert_eq!(label.name, "bug");
        assert_eq!(label.description, "Something that doesn't work");
        assert_eq!(label.color.to_string(), "#ff0000");
    }
}
