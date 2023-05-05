use radicle::crypto::PublicKey;
use radicle::git;
use radicle::git::refs::storage::{IDENTITY_BRANCH, SIGREFS_BRANCH};
use radicle::storage::git::NAMESPACES_GLOB;
use radicle::storage::Namespaces;

use super::Refspec;

/// Radicle special refs, i.e. `refs/rad/*`.
pub struct SpecialRefs(pub(super) Namespaces);

impl SpecialRefs {
    pub fn into_refspecs(self) -> Vec<Refspec> {
        match &self.0 {
            Namespaces::All => {
                let id = NAMESPACES_GLOB.join(&*IDENTITY_BRANCH);
                let sigrefs = NAMESPACES_GLOB.join(&*SIGREFS_BRANCH);

                [id, sigrefs]
                    .into_iter()
                    .map(|spec| Refspec {
                        src: spec.clone(),
                        dst: spec,
                        force: true,
                    })
                    .collect()
            }
            Namespaces::Trusted(pks) => pks.iter().flat_map(rad_refs).collect(),
        }
    }
}

fn rad_refs(pk: &PublicKey) -> Vec<Refspec> {
    let ns = pk.to_namespace();
    let id = git::PatternString::from(ns.join(&*IDENTITY_BRANCH));
    let id = Refspec {
        src: id.clone(),
        dst: id,
        force: true,
    };
    let sigrefs = git::PatternString::from(ns.join(&*SIGREFS_BRANCH));
    let sigrefs = Refspec {
        src: sigrefs.clone(),
        dst: sigrefs,
        force: true,
    };
    vec![id, sigrefs]
}
