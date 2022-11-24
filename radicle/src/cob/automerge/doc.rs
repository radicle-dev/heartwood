use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::Hash;
use std::ops::Deref;
use std::str::FromStr;

use automerge::{Automerge, AutomergeError, ObjType};

use crate::cob::automerge::value::{FromValue, ValueError};

/// Error decoding a document.
#[derive(thiserror::Error, Debug)]
pub enum DocumentError {
    #[error(transparent)]
    Automerge(#[from] AutomergeError),
    #[error("property '{0}' not found in object")]
    PropertyNotFound(automerge::Prop),
    #[error("error decoding property `{0}`")]
    Property(String),
    #[error("property '{0}' is not an object")]
    NotAnObject(automerge::Prop),
    #[error("Unexpected object type: expected `{expected}`, got `{actual}`")]
    ObjectType {
        expected: automerge::ObjType,
        actual: automerge::ObjType,
    },
    #[error("error decoding value: {0}")]
    Value(#[from] ValueError),
    #[error("list under `{0}` cannot be empty")]
    EmptyList(&'static str),
}

/// Automerge document decoder.
///
/// Wraps a document, providing convenience functions. Derefs to the underlying doc.
#[derive(Copy, Clone)]
pub struct Document<'a> {
    doc: &'a Automerge,
}

impl<'a> Document<'a> {
    /// Create a new document from an automerge document.
    pub fn new(doc: &'a Automerge) -> Self {
        Self { doc }
    }

    /// Get the value of a property of an object.
    pub fn get<O: AsRef<automerge::ObjId>, P: Into<automerge::Prop>, T: FromValue<'a>>(
        &self,
        id: O,
        prop: P,
    ) -> Result<T, DocumentError> {
        let prop = prop.into();
        let (val, _) = Document::get_raw(self, id, prop)?;

        T::from_value(val).map_err(DocumentError::from)
    }

    /// Get an object's raw property.
    pub fn get_raw<O: AsRef<automerge::ObjId>, P: Into<automerge::Prop>>(
        &self,
        id: O,
        prop: P,
    ) -> Result<(automerge::Value<'a>, automerge::ObjId), DocumentError> {
        let prop = prop.into();

        self.doc
            .get(id.as_ref(), prop.clone())?
            .ok_or(DocumentError::PropertyNotFound(prop))
    }

    /// Get the id of an object's property.
    pub fn get_id<O: AsRef<automerge::ObjId>, P: Into<automerge::Prop>>(
        &self,
        id: O,
        prop: P,
    ) -> Result<automerge::ObjId, DocumentError> {
        self.get_raw(id, prop).map(|(_, id)| id)
    }

    /// Get a property using a lookup function.
    pub fn lookup<V, O: AsRef<automerge::ObjId>, P: Into<automerge::Prop>>(
        &self,
        id: O,
        prop: P,
        lookup: fn(Document, &automerge::ObjId) -> Result<V, DocumentError>,
    ) -> Result<V, DocumentError> {
        let obj_id = self.get_id(&id, prop)?;
        lookup(*self, &obj_id)
    }

    /// Get a list-like value from an object.
    pub fn list<V, O: AsRef<automerge::ObjId>, P: Into<automerge::Prop>>(
        &self,
        id: O,
        prop: P,
        item: fn(Document, &automerge::ObjId) -> Result<V, DocumentError>,
    ) -> Result<Vec<V>, DocumentError> {
        let prop = prop.into();
        let id = id.as_ref();

        let Some((list, list_id)) = self.doc.get(id, prop.clone())? else {
            return Err(DocumentError::PropertyNotFound(prop));
        };
        let Some(objtype) = list.to_objtype() else {
            return Err(DocumentError::NotAnObject(prop));
        };
        if objtype != ObjType::List {
            return Err(DocumentError::ObjectType {
                expected: ObjType::List,
                actual: objtype,
            });
        }

        let mut objs: Vec<V> = Vec::new();
        for i in 0..self.length(&list_id) {
            let Some((_, item_id)) = self.doc.get(&list_id, i as usize)? else {
                return Err(DocumentError::PropertyNotFound(prop));
            };
            let item = item(*self, &item_id)?;

            objs.push(item);
        }
        Ok(objs)
    }

    /// Get a map-like value from an object.
    pub fn map<
        V: Default,
        K: Hash + Eq + FromStr,
        O: AsRef<automerge::ObjId>,
        P: Into<automerge::Prop>,
    >(
        &self,
        id: O,
        prop: P,
        mut value: impl FnMut(&mut V),
    ) -> Result<HashMap<K, V>, DocumentError> {
        let prop = prop.into();
        let id = id.as_ref();

        let Some((obj, obj_id)) = self.doc.get(id, prop.clone())? else {
            return Err(DocumentError::PropertyNotFound(prop))?;
        };
        let Some(objtype) = obj.to_objtype() else {
            return Err(DocumentError::NotAnObject(prop));
        };
        if objtype != ObjType::Map {
            return Err(DocumentError::ObjectType {
                expected: ObjType::Map,
                actual: objtype,
            });
        }

        let mut map = HashMap::new();
        for key in self.doc.keys(&obj_id) {
            let key = K::from_str(&key).map_err(|_| DocumentError::Property(key))?;
            let val = map.entry(key).or_default();

            value(val);
        }
        Ok(map)
    }

    /// Get a folded value out of an object.
    pub fn fold<
        T: Default,
        V: FromValue<'a> + fmt::Debug,
        O: AsRef<automerge::ObjId>,
        P: Into<automerge::Prop>,
    >(
        &self,
        id: O,
        prop: P,
        mut f: impl FnMut(&mut T, V),
    ) -> Result<T, DocumentError> {
        let prop = prop.into();
        let id = id.as_ref();

        let Some((obj, obj_id)) = self.doc.get(id, prop.clone())? else {
            return Err(DocumentError::PropertyNotFound(prop));
        };
        let Some(objtype) = obj.to_objtype() else {
            return Err(DocumentError::NotAnObject(prop));
        };
        if objtype != ObjType::List {
            return Err(DocumentError::ObjectType {
                expected: ObjType::List,
                actual: objtype,
            });
        }

        let mut acc = T::default();
        for i in 0..self.doc.length(&obj_id) {
            let Some((item, _)) = self.doc.get(&obj_id, i as usize)? else {
                return Err(DocumentError::PropertyNotFound(prop));
            };
            let val = V::from_value(item)?;

            f(&mut acc, val);
        }
        Ok(acc)
    }

    /// Get the keys of a map-like property.
    pub fn keys<K: Hash + Eq + FromStr, O: AsRef<automerge::ObjId>, P: Into<automerge::Prop>>(
        &self,
        id: O,
        prop: P,
    ) -> Result<HashSet<K>, DocumentError> {
        let prop = prop.into();
        let id = id.as_ref();

        let Some((obj, obj_id)) = self.doc.get(id, prop.clone())? else {
            return Err(DocumentError::PropertyNotFound(prop));
        };
        let Some(objtype) = obj.to_objtype() else {
            return Err(DocumentError::NotAnObject(prop));
        };
        if objtype != ObjType::Map {
            return Err(DocumentError::ObjectType {
                expected: ObjType::Map,
                actual: objtype,
            });
        }

        let mut keys = HashSet::new();
        for key in self.doc.keys(&obj_id) {
            let key = K::from_str(&key).map_err(|_| DocumentError::Property(key))?;

            keys.insert(key);
        }
        Ok(keys)
    }
}

impl<'a> Deref for Document<'a> {
    type Target = Automerge;

    fn deref(&self) -> &Self::Target {
        self.doc
    }
}
