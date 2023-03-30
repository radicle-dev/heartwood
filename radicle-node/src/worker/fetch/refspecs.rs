use std::fmt;
use std::fmt::Write as _;

use radicle::crypto::PublicKey;
use radicle::git;
use radicle::git::refs::storage::{IDENTITY_BRANCH, SIGREFS_BRANCH};
use radicle::storage;
use radicle::storage::git::NAMESPACES_GLOB;
use radicle::storage::{Namespaces, Remote};

/// A Git [refspec].
///
/// [refspec]: https://git-scm.com/book/en/v2/Git-Internals-The-Refspec
// TODO(finto): this should go into radicle-git-ext/git-ref-format
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refspec<T, U> {
    pub src: T,
    pub dst: U,
    pub force: bool,
}

impl<T, U> fmt::Display for Refspec<T, U>
where
    T: AsRef<git::PatternStr>,
    U: AsRef<git::PatternStr>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.force {
            f.write_char('+')?;
        }
        write!(f, "{}:{}", self.src.as_ref(), self.dst.as_ref())
    }
}

/// Radicle special refs, i.e. `refs/rad/*`.
pub struct SpecialRefs(pub(super) Namespaces);

impl AsRefspecs for SpecialRefs {
    fn as_refspecs(&self) -> Vec<Refspec<git::PatternString, git::PatternString>> {
        match &self.0 {
            Namespaces::All => {
                let id = NAMESPACES_GLOB.join(&*IDENTITY_BRANCH);
                let sigrefs = NAMESPACES_GLOB.join(&*SIGREFS_BRANCH);
                vec![
                    Refspec {
                        src: id.clone(),
                        dst: id,
                        force: false,
                    },
                    Refspec {
                        src: sigrefs.clone(),
                        dst: sigrefs,
                        force: false,
                    },
                ]
            }
            Namespaces::Trusted(pks) => pks.iter().flat_map(rad_refs).collect(),
        }
    }

    fn into_refspecs(self) -> Vec<Refspec<git::PatternString, git::PatternString>> {
        self.as_refspecs()
    }
}

fn rad_refs(pk: &PublicKey) -> Vec<Refspec<git::PatternString, git::PatternString>> {
    let ns = pk.to_namespace();
    let id = git::PatternString::from(ns.join(&*IDENTITY_BRANCH));
    let id = Refspec {
        src: id.clone(),
        dst: id,
        force: false,
    };
    let sigrefs = git::PatternString::from(ns.join(&*SIGREFS_BRANCH));
    let sigrefs = Refspec {
        src: sigrefs.clone(),
        dst: sigrefs,
        force: false,
    };
    vec![id, sigrefs]
}

/// A conversion trait for producing a set of Git [`Refspec`]s.
pub trait AsRefspecs
where
    Self: Sized,
{
    /// Convert the borrowed data into a set of [`Refspec`]s.
    fn as_refspecs(&self) -> Vec<Refspec<git::PatternString, git::PatternString>>;

    /// Convert the owned data into a set of [`Refspec`]s.
    ///
    /// Nb. The default implementation uses
    /// [`AsRefspecs::as_refspecs`], which may clone data.
    fn into_refspecs(self) -> Vec<Refspec<git::PatternString, git::PatternString>> {
        self.as_refspecs()
    }
}

impl AsRefspecs for Namespaces {
    fn as_refspecs(&self) -> Vec<Refspec<git::PatternString, git::PatternString>> {
        match self {
            Namespaces::All => vec![Refspec {
                src: (*storage::git::NAMESPACES_GLOB).clone(),
                dst: (*storage::git::NAMESPACES_GLOB).clone(),
                force: false,
            }],
            Namespaces::Trusted(pks) => pks
                .iter()
                .map(|pk| {
                    let ns = pk.to_namespace().with_pattern(git::refspec::STAR);
                    Refspec {
                        src: ns.clone(),
                        dst: ns,
                        force: false,
                    }
                })
                .collect(),
        }
    }
}

impl AsRefspecs for Refspec<git::PatternString, git::PatternString> {
    fn as_refspecs(&self) -> Vec<Refspec<git::PatternString, git::PatternString>> {
        vec![self.clone()]
    }

    fn into_refspecs(self) -> Vec<Refspec<git::PatternString, git::PatternString>> {
        vec![self]
    }
}

impl<T: AsRefspecs> AsRefspecs for Vec<T> {
    fn as_refspecs(&self) -> Vec<Refspec<git::PatternString, git::PatternString>> {
        self.iter().flat_map(AsRefspecs::as_refspecs).collect()
    }

    fn into_refspecs(self) -> Vec<Refspec<git::PatternString, git::PatternString>> {
        self.into_iter()
            .flat_map(AsRefspecs::into_refspecs)
            .collect()
    }
}

impl AsRefspecs for Remote {
    fn as_refspecs(&self) -> Vec<Refspec<git::PatternString, git::PatternString>> {
        let ns = self.id.to_namespace();
        // Nb. the references in Refs are expected to be Qualified
        self.refs
            .iter()
            .map(|(name, _)| {
                let name = git::PatternString::from(ns.join(name));
                Refspec {
                    src: name.clone(),
                    dst: name,
                    force: true,
                }
            })
            .collect()
    }
}
