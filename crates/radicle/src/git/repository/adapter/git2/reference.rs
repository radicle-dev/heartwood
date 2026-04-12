use radicle_git_ref_format::{Qualified, RefStr, refspec};
use radicle_oid::Oid;

use crate::git;
use crate::git::raw;
use crate::git::repository::reference::error::{read, write};
use crate::git::repository::reference::{self, symbolic};

use super::NotFound as _;

/// Iterator adapter for [`reference::Reader::list_refs`].
pub struct References<'a> {
    inner: git::raw::References<'a>,
}

impl Iterator for References<'_> {
    type Item = Result<(Qualified<'static>, Oid), read::ListReference>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let r = match self.inner.next()? {
                Ok(r) => r,
                Err(e) => {
                    return Some(Err(read::ListReference::backend(e)));
                }
            };

            let name = match r.name() {
                Some(n) => n,
                None => continue,
            };

            let refstr = match RefStr::try_from_str(name) {
                Ok(r) => r,
                Err(e) => {
                    return Some(Err(read::ListReference::Parse {
                        name: name.to_string(),
                        source: e,
                    }));
                }
            };

            let qualified = match Qualified::from_refstr(refstr) {
                Some(q) => q.to_owned(),
                None => continue,
            };

            let oid = match r.resolve().map(|r| r.target()) {
                Ok(Some(oid)) => Oid::from(oid),
                Ok(None) => continue,
                Err(e) => {
                    return Some(Err(read::ListReference::Peel {
                        name: qualified,
                        source: Box::new(e),
                    }));
                }
            };

            return Some(Ok((qualified, oid)));
        }
    }
}

impl reference::Reader for raw::Repository {
    type References<'a> = References<'a>;

    fn ref_target<R: AsRef<RefStr>>(&self, name: &R) -> Result<Option<Oid>, read::RefTarget> {
        self.refname_to_id(name.as_ref().as_str())
            .map(Oid::from)
            .or_is_not_found()
            .map_err(read::RefTarget::backend)
    }

    fn list_refs<'a, P>(&'a self, pattern: &P) -> Result<Self::References<'a>, read::ListRefs>
    where
        P: AsRef<refspec::PatternStr>,
    {
        let inner = self
            .references_glob(pattern.as_ref().as_str())
            .map_err(read::ListRefs::backend)?;
        Ok(References { inner })
    }
}

impl reference::Writer for raw::Repository {
    fn write_ref<R>(
        &self,
        name: &R,
        target: reference::Target,
        reflog: &str,
    ) -> Result<(), write::WriteRef>
    where
        R: AsRef<RefStr>,
    {
        let name = name.as_ref();

        // Verify the target object exists.
        {
            let odb = self.odb().map_err(write::WriteRef::backend)?;
            let target_oid = target.target();
            if !odb.exists(target_oid.into()) {
                return Err(write::WriteRef::MissingTarget {
                    name: name.to_string(),
                    target: target_oid,
                });
            }
        }

        match target {
            reference::Target::Create { target } => {
                create_reference(self, reflog, name, target)?;
            }
            reference::Target::Upsert { target } => {
                upsert_reference(self, reflog, name, target)?;
            }
            reference::Target::Cas { target, expected } => {
                cas_reference(self, reflog, name, target, expected)?;
            }
        }

        Ok(())
    }

    fn delete_ref<R>(&self, name: &R) -> Result<(), write::DeleteRef>
    where
        R: AsRef<RefStr>,
    {
        match self.find_reference(name.as_ref().as_str()) {
            Ok(mut r) => r.delete().map_err(write::DeleteRef::backend),
            Err(e) if matches!(e.code(), git::raw::ErrorCode::NotFound) => Ok(()),
            Err(e) => Err(write::DeleteRef::backend(e)),
        }
    }
}

fn create_reference(
    repository: &git::raw::Repository,
    reflog: &str,
    name: &RefStr,
    target: Oid,
) -> Result<(), write::WriteRef> {
    repository
        .reference(name, target.into(), false, reflog)
        .map_err(|e| {
            if matches!(e.code(), raw::ErrorCode::Exists) {
                write::WriteRef::ReferenceExists {
                    name: name.to_string(),
                }
            } else {
                write::WriteRef::backend(e)
            }
        })?;
    Ok(())
}

fn upsert_reference(
    repository: &git::raw::Repository,
    reflog: &str,
    name: &RefStr,
    target: Oid,
) -> Result<(), write::WriteRef> {
    repository
        .reference(name, target.into(), true, reflog)
        .map_err(write::WriteRef::backend)?;
    Ok(())
}

fn cas_reference(
    repository: &git::raw::Repository,
    reflog: &str,
    name: &RefStr,
    target: Oid,
    expected: Oid,
) -> Result<(), write::WriteRef> {
    // CAS requires `force=true` so that libgit2 skips the existence
    // check in `reference_path_available` and instead compares the
    // current value via `cmp_old_ref`.  With `force=false`, an existing
    // reference would always fail with `GIT_EEXISTS` before the old
    // value is ever compared.
    repository
        .reference_matching(name, target.into(), true, expected.into(), reflog)
        .map_err(|e| {
            if matches!(e.code(), raw::ErrorCode::Modified) {
                write::WriteRef::CasFailed {
                    name: name.to_string(),
                    expected,
                }
            } else {
                write::WriteRef::backend(e)
            }
        })?;
    Ok(())
}

impl symbolic::Writer for raw::Repository {
    fn write_symbolic_ref<R>(
        &self,
        name: &R,
        target: symbolic::Target,
        reflog: &str,
    ) -> Result<(), write::WriteSymbolicRef>
    where
        R: AsRef<RefStr>,
    {
        let name = name.as_ref();

        // Ensure the target reference exists.
        {
            let target = target.target();
            match self.find_reference(target) {
                Ok(_) => {}
                Err(e) if matches!(e.code(), git::raw::ErrorCode::NotFound) => {
                    return Err(write::WriteSymbolicRef::MissingTarget {
                        name: name.to_ref_string(),
                        target: target.to_owned(),
                    });
                }
                Err(e) => {
                    return Err(write::WriteSymbolicRef::backend(e));
                }
            }
        }

        match target {
            symbolic::Target::Create { target } => {
                create_symbolic_reference(self, reflog, name, &target)?;
            }
            symbolic::Target::Upsert { target } => {
                upsert_symbolic_reference(self, reflog, name, &target)?;
            }
            symbolic::Target::Cas { target, expected } => {
                cas_symbolic_reference(self, reflog, name, &target, &expected)?;
            }
        }

        Ok(())
    }
}

fn create_symbolic_reference(
    repository: &git::raw::Repository,
    reflog: &str,
    name: &RefStr,
    target: &RefStr,
) -> Result<(), write::WriteSymbolicRef> {
    repository
        .reference_symbolic(name, target, false, reflog)
        .map_err(|e| {
            if matches!(e.code(), raw::ErrorCode::Exists) {
                write::WriteSymbolicRef::ReferenceExists {
                    name: name.to_ref_string(),
                    target: target.to_ref_string(),
                }
            } else {
                write::WriteSymbolicRef::backend(e)
            }
        })?;
    Ok(())
}

fn upsert_symbolic_reference(
    repository: &git::raw::Repository,
    reflog: &str,
    name: &RefStr,
    target: &RefStr,
) -> Result<(), write::WriteSymbolicRef> {
    repository
        .reference_symbolic(name, target, true, reflog)
        .map_err(write::WriteSymbolicRef::backend)?;
    Ok(())
}

fn cas_symbolic_reference(
    repository: &git::raw::Repository,
    reflog: &str,
    name: &RefStr,
    target: &RefStr,
    expected: &RefStr,
) -> Result<(), write::WriteSymbolicRef> {
    // See `cas_reference` for why `force=true` is required for CAS.
    repository
        .reference_symbolic_matching(name, target, true, expected, reflog)
        .map_err(|e| {
            if matches!(e.code(), raw::ErrorCode::Modified) {
                write::WriteSymbolicRef::CasFailed {
                    name: name.to_ref_string(),
                    expected: expected.to_ref_string(),
                }
            } else {
                write::WriteSymbolicRef::backend(e)
            }
        })?;
    Ok(())
}
