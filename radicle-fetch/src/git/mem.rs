use std::collections::HashMap;

use radicle::git::{Component, Oid, Qualified, RefString};
use radicle::node::NodeId;

use super::refs::{Applied, RefUpdate, Update};

/// An in-memory reference store.
///
/// It provides the same functionality as the [`super::Refdb`], but is
/// used to temporarily store reference names and objects.
#[derive(Clone, Debug, Default)]
pub struct Refdb(HashMap<Qualified<'static>, Oid>);

impl Refdb {
    pub fn refname_to_id<'a, N>(&self, refname: N) -> Option<Oid>
    where
        N: Into<Qualified<'a>>,
    {
        let name = refname.into();
        self.0.get(&name).copied()
    }

    pub fn references_of<'a>(
        &'a self,
        remote: &'a NodeId,
    ) -> impl Iterator<Item = (RefString, Oid)> + 'a {
        self.0.iter().filter_map(move |(refname, oid)| {
            let ns = refname.to_namespaced()?;
            (ns.namespace() == Component::from(remote))
                .then(|| (ns.strip_namespace().to_ref_string(), *oid))
        })
    }

    pub fn update<'a, I>(&mut self, updates: I) -> Applied<'a>
    where
        I: IntoIterator<Item = Update<'a>>,
    {
        updates
            .into_iter()
            .fold(Applied::default(), |mut ap, update| match update {
                Update::Direct { name, target, .. } => {
                    let name = name.into_qualified().into_owned();
                    let prev = match self.0.insert(name.clone(), target) {
                        Some(prev) => prev,
                        None => radicle::git::raw::Oid::zero().into(),
                    };
                    ap.updated.push(RefUpdate::Updated {
                        name: name.to_ref_string(),
                        old: prev,
                        new: target,
                    });
                    ap
                }
                Update::Prune { name, .. } => {
                    let name = name.into_qualified().into_owned();
                    if let Some((name, prev)) = self.0.remove_entry(&name) {
                        ap.updated.push(RefUpdate::Deleted {
                            name: name.to_ref_string(),
                            oid: prev,
                        })
                    }
                    ap
                }
            })
    }

    #[allow(dead_code)]
    pub(crate) fn inspect(&self) {
        if self.0.is_empty() {
            println!("Refdb is empty!");
        } else {
            for (name, oid) in self.0.iter() {
                println!("{name} -> {oid}");
            }
        }
    }
}
