use radicle_git_ref_format::{Qualified, RefStr, refspec};
use radicle_oid::Oid;

use crate::git;
use crate::git::raw;
use crate::git::repository::reference::error;
use crate::git::repository::reference::{self, symbolic};

use super::NotFound as _;

/// Iterator adapter for [`reference::Reader::list_refs`].
pub struct References<'a> {
    inner: git::raw::References<'a>,
}

impl Iterator for References<'_> {
    type Item = Result<(Qualified<'static>, Oid), error::read::ListReference>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let r = match self.inner.next()? {
                Ok(r) => r,
                Err(e) => {
                    return Some(Err(error::read::ListReference::backend(e)));
                }
            };

            let name = match r.name() {
                Some(n) => n,
                None => continue,
            };

            let refstr = match RefStr::try_from_str(name) {
                Ok(r) => r,
                Err(e) => {
                    return Some(Err(error::read::ListReference::Parse {
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
                    return Some(Err(error::read::ListReference::Peel {
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

    fn ref_target<R: AsRef<RefStr>>(
        &self,
        name: &R,
    ) -> Result<Option<Oid>, error::read::RefTarget> {
        self.refname_to_id(name.as_ref().as_str())
            .map(Oid::from)
            .or_is_not_found()
            .map_err(error::read::RefTarget::backend)
    }

    fn list_refs<'a, P>(
        &'a self,
        pattern: &P,
    ) -> Result<Self::References<'a>, error::read::ListRefs>
    where
        P: AsRef<refspec::PatternStr>,
    {
        let inner = self
            .references_glob(pattern.as_ref().as_str())
            .map_err(error::read::ListRefs::backend)?;
        Ok(References { inner })
    }
}

impl reference::Writer for raw::Repository {
    fn write_ref<R>(
        &self,
        name: &R,
        target: reference::Target,
        reflog: &str,
    ) -> Result<(), error::write::WriteRef>
    where
        R: AsRef<RefStr>,
    {
        let name = name.as_ref();

        // Verify the target object exists.
        {
            let odb = self.odb().map_err(error::write::WriteRef::backend)?;
            let target_oid = target.target();
            if !odb.exists(target_oid.into()) {
                return Err(error::write::WriteRef::MissingTarget {
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

    fn delete_ref<R>(&self, name: &R) -> Result<(), error::write::DeleteRef>
    where
        R: AsRef<RefStr>,
    {
        match self.find_reference(name.as_ref().as_str()) {
            Ok(mut r) => r.delete().map_err(error::write::DeleteRef::backend),
            Err(e) if matches!(e.code(), git::raw::ErrorCode::NotFound) => Ok(()),
            Err(e) => Err(error::write::DeleteRef::backend(e)),
        }
    }
}

fn create_reference(
    repository: &git::raw::Repository,
    reflog: &str,
    name: &RefStr,
    target: Oid,
) -> Result<(), error::write::WriteRef> {
    repository
        .reference(name, target.into(), false, reflog)
        .map_err(|e| {
            if matches!(e.code(), raw::ErrorCode::Exists) {
                error::write::WriteRef::ReferenceExists {
                    name: name.to_string(),
                }
            } else {
                error::write::WriteRef::backend(e)
            }
        })?;
    Ok(())
}

fn upsert_reference(
    repository: &git::raw::Repository,
    reflog: &str,
    name: &RefStr,
    target: Oid,
) -> Result<(), error::write::WriteRef> {
    repository
        .reference(name, target.into(), true, reflog)
        .map_err(error::write::WriteRef::backend)?;
    Ok(())
}

fn cas_reference(
    repository: &git::raw::Repository,
    reflog: &str,
    name: &RefStr,
    target: Oid,
    expected: Oid,
) -> Result<(), error::write::WriteRef> {
    // CAS requires `force=true` so that libgit2 skips the existence
    // check in `reference_path_available` and instead compares the
    // current value via `cmp_old_ref`.  With `force=false`, an existing
    // reference would always fail with `GIT_EEXISTS` before the old
    // value is ever compared.
    repository
        .reference_matching(name, target.into(), true, expected.into(), reflog)
        .map_err(|e| {
            if matches!(e.code(), raw::ErrorCode::Modified) {
                error::write::WriteRef::CasFailed {
                    name: name.to_string(),
                    expected,
                }
            } else {
                error::write::WriteRef::backend(e)
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
    ) -> Result<(), error::write::WriteSymbolicRef>
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
                    return Err(error::write::WriteSymbolicRef::MissingTarget {
                        name: name.to_ref_string(),
                        target: target.to_owned(),
                    });
                }
                Err(e) => {
                    return Err(error::write::WriteSymbolicRef::backend(e));
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
) -> Result<(), error::write::WriteSymbolicRef> {
    repository
        .reference_symbolic(name, target, false, reflog)
        .map_err(|e| {
            if matches!(e.code(), raw::ErrorCode::Exists) {
                error::write::WriteSymbolicRef::ReferenceExists {
                    name: name.to_ref_string(),
                    target: target.to_ref_string(),
                }
            } else {
                error::write::WriteSymbolicRef::backend(e)
            }
        })?;
    Ok(())
}

fn upsert_symbolic_reference(
    repository: &git::raw::Repository,
    reflog: &str,
    name: &RefStr,
    target: &RefStr,
) -> Result<(), error::write::WriteSymbolicRef> {
    repository
        .reference_symbolic(name, target, true, reflog)
        .map_err(error::write::WriteSymbolicRef::backend)?;
    Ok(())
}

fn cas_symbolic_reference(
    repository: &git::raw::Repository,
    reflog: &str,
    name: &RefStr,
    target: &RefStr,
    expected: &RefStr,
) -> Result<(), error::write::WriteSymbolicRef> {
    // See `cas_reference` for why `force=true` is required for CAS.
    repository
        .reference_symbolic_matching(name, target, true, expected, reflog)
        .map_err(|e| {
            if matches!(e.code(), raw::ErrorCode::Modified) {
                error::write::WriteSymbolicRef::CasFailed {
                    name: name.to_ref_string(),
                    expected: expected.to_ref_string(),
                }
            } else {
                error::write::WriteSymbolicRef::backend(e)
            }
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use tempfile::TempDir;

    use radicle_git_ref_format::refname;
    use radicle_oid::Oid;

    use crate::git::raw::Repository;
    use crate::git::repository::object::Writer as ObjectWriter;
    use crate::git::repository::reference::error::write::WriteRef;
    use crate::git::repository::reference::{Reader, Target, Writer};

    /// A helper environment that provides an isolated Git repository
    /// and a couple of valid object IDs (OIDs) to use as reference targets.
    struct TestEnv {
        _tempdir: TempDir,
        pub repo: Repository,
        pub oid1: Oid,
        pub oid2: Oid,
    }

    impl TestEnv {
        fn new() -> Self {
            let tempdir = tempfile::tempdir().expect("failed to create temp dir");
            let repo = Repository::init(tempdir.path()).expect("failed to init git repo");

            let oid1 = repo
                .write_blob(b"blob 1 content")
                .expect("failed to write blob 1");
            let oid2 = repo
                .write_blob(b"blob 2 content")
                .expect("failed to write blob 2");

            Self {
                _tempdir: tempdir,
                repo,
                oid1,
                oid2,
            }
        }
    }

    mod direct {
        use super::*;

        mod create {
            use super::*;

            #[test]
            fn missing_target_returns_error() {
                let env = TestEnv::new();
                let missing_oid = Oid::from_str("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef")
                    .expect("failed to parse oid");
                let refname = refname!("refs/heads/missing");

                let err = env
                    .repo
                    .write_ref(
                        &refname,
                        Target::Create {
                            target: missing_oid,
                        },
                        "test",
                    )
                    .unwrap_err();

                assert!(matches!(err, WriteRef::MissingTarget { .. }));
            }

            #[test]
            fn with_new_ref_succeeds() {
                let env = TestEnv::new();
                let refname = refname!("refs/heads/new_ref");

                env.repo
                    .write_ref(&refname, Target::Create { target: env.oid1 }, "test")
                    .unwrap();

                let target = env.repo.ref_target(&refname).unwrap();
                assert_eq!(target, Some(env.oid1));
            }

            #[test]
            fn with_existing_ref_returns_error() {
                let env = TestEnv::new();
                let refname = refname!("refs/heads/existing_ref");

                env.repo
                    .write_ref(&refname, Target::Create { target: env.oid1 }, "test")
                    .unwrap();

                let err = env
                    .repo
                    .write_ref(&refname, Target::Create { target: env.oid2 }, "test")
                    .unwrap_err();

                assert!(matches!(err, WriteRef::ReferenceExists { .. }));
            }
        }

        mod upsert {
            use super::*;

            #[test]
            fn with_new_ref_succeeds() {
                let env = TestEnv::new();
                let refname = refname!("refs/heads/upsert_new");

                env.repo
                    .write_ref(&refname, Target::Upsert { target: env.oid1 }, "test")
                    .unwrap();

                let target = env.repo.ref_target(&refname).unwrap();
                assert_eq!(target, Some(env.oid1));
            }

            #[test]
            fn with_existing_ref_updates_target() {
                let env = TestEnv::new();
                let refname = refname!("refs/heads/upsert_existing");

                env.repo
                    .write_ref(&refname, Target::Create { target: env.oid1 }, "test")
                    .unwrap();

                env.repo
                    .write_ref(&refname, Target::Upsert { target: env.oid2 }, "test")
                    .unwrap();

                let target = env.repo.ref_target(&refname).unwrap();
                assert_eq!(target, Some(env.oid2));
            }
        }

        mod cas {
            use super::*;

            #[test]
            fn missing_target_returns_error() {
                let env = TestEnv::new();
                let missing_oid = Oid::from_str("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef")
                    .expect("failed to parse oid");
                let refname = refname!("refs/heads/missing");

                let err = env
                    .repo
                    .write_ref(
                        &refname,
                        Target::Cas {
                            target: missing_oid,
                            expected: missing_oid,
                        },
                        "test",
                    )
                    .unwrap_err();

                assert!(matches!(err, WriteRef::MissingTarget { .. }));
            }

            #[test]
            fn with_matching_expected_target_succeeds() {
                let env = TestEnv::new();
                let refname = refname!("refs/heads/cas_match");

                env.repo
                    .write_ref(&refname, Target::Create { target: env.oid1 }, "test")
                    .unwrap();

                env.repo
                    .write_ref(
                        &refname,
                        Target::Cas {
                            target: env.oid2,
                            expected: env.oid1,
                        },
                        "test",
                    )
                    .unwrap();

                let target = env.repo.ref_target(&refname).unwrap();
                assert_eq!(target, Some(env.oid2));
            }

            #[test]
            fn with_mismatched_expected_target_returns_error() {
                let env = TestEnv::new();
                let refname = refname!("refs/heads/cas_mismatch");

                env.repo
                    .write_ref(&refname, Target::Create { target: env.oid1 }, "test")
                    .unwrap();

                // Attempt to CAS to oid2, but incorrectly expect it to currently be oid2 (it is oid1)
                let err = env
                    .repo
                    .write_ref(
                        &refname,
                        Target::Cas {
                            target: env.oid2,
                            expected: env.oid2,
                        },
                        "test",
                    )
                    .unwrap_err();

                assert!(matches!(err, WriteRef::CasFailed { .. }));

                // Verify the reference was not changed
                let target = env.repo.ref_target(&refname).unwrap();
                assert_eq!(target, Some(env.oid1));
            }
        }

        mod delete {
            use super::*;

            #[test]
            fn with_existing_ref_succeeds() {
                let env = TestEnv::new();
                let refname = refname!("refs/heads/delete_existing");

                env.repo
                    .write_ref(&refname, Target::Create { target: env.oid1 }, "test")
                    .unwrap();

                env.repo.delete_ref(&refname).unwrap();

                let target = env.repo.ref_target(&refname).unwrap();
                assert_eq!(target, None);
            }

            #[test]
            fn with_non_existent_ref_succeeds_idempotently() {
                let env = TestEnv::new();
                let refname = refname!("refs/heads/delete_non_existent");

                // Should not return an error even though the ref doesn't exist
                env.repo.delete_ref(&refname).unwrap();
            }
        }
    }

    mod symbolic {
        use super::*;
        use crate::git::repository::reference::error::write::WriteSymbolicRef;
        use crate::git::repository::reference::symbolic::{
            Target as SymTarget, Writer as SymWriter,
        };

        mod target_validation {
            use super::*;

            #[test]
            fn missing_target_returns_error() {
                let env = TestEnv::new();
                let refname = refname!("refs/heads/sym");
                let missing_target = refname!("refs/heads/missing");

                let err = env
                    .repo
                    .write_symbolic_ref(&refname, SymTarget::create(&missing_target), "test")
                    .unwrap_err();

                assert!(matches!(err, WriteSymbolicRef::MissingTarget { .. }));
            }
        }

        mod create {
            use super::*;

            #[test]
            fn with_new_ref_succeeds() {
                let env = TestEnv::new();
                let target_ref = refname!("refs/heads/target");
                let sym_ref = refname!("refs/heads/sym");

                env.repo
                    .write_ref(&target_ref, Target::Create { target: env.oid1 }, "test")
                    .unwrap();

                env.repo
                    .write_symbolic_ref(&sym_ref, SymTarget::create(&target_ref), "test")
                    .unwrap();

                let target = env.repo.ref_target(&sym_ref).unwrap();
                assert_eq!(target, Some(env.oid1));
            }

            #[test]
            fn with_existing_ref_returns_error() {
                let env = TestEnv::new();
                let target_ref = refname!("refs/heads/target");
                let sym_ref = refname!("refs/heads/sym");

                env.repo
                    .write_ref(&target_ref, Target::Create { target: env.oid1 }, "test")
                    .unwrap();

                env.repo
                    .write_symbolic_ref(&sym_ref, SymTarget::create(&target_ref), "test")
                    .unwrap();

                let err = env
                    .repo
                    .write_symbolic_ref(&sym_ref, SymTarget::create(&target_ref), "test")
                    .unwrap_err();

                assert!(matches!(err, WriteSymbolicRef::ReferenceExists { .. }));
            }
        }

        mod upsert {
            use super::*;

            #[test]
            fn with_new_ref_succeeds() {
                let env = TestEnv::new();
                let target_ref = refname!("refs/heads/target");
                let sym_ref = refname!("refs/heads/sym");

                env.repo
                    .write_ref(&target_ref, Target::Create { target: env.oid1 }, "test")
                    .unwrap();

                env.repo
                    .write_symbolic_ref(&sym_ref, SymTarget::upsert(&target_ref), "test")
                    .unwrap();

                let target = env.repo.ref_target(&sym_ref).unwrap();
                assert_eq!(target, Some(env.oid1));
            }

            #[test]
            fn with_existing_ref_updates_target() {
                let env = TestEnv::new();
                let target1 = refname!("refs/heads/target1");
                let target2 = refname!("refs/heads/target2");
                let sym_ref = refname!("refs/heads/sym");

                env.repo
                    .write_ref(&target1, Target::Create { target: env.oid1 }, "test")
                    .unwrap();
                env.repo
                    .write_ref(&target2, Target::Create { target: env.oid2 }, "test")
                    .unwrap();

                env.repo
                    .write_symbolic_ref(&sym_ref, SymTarget::create(&target1), "test")
                    .unwrap();

                env.repo
                    .write_symbolic_ref(&sym_ref, SymTarget::upsert(&target2), "test")
                    .unwrap();

                let target = env.repo.ref_target(&sym_ref).unwrap();
                assert_eq!(target, Some(env.oid2));
            }
        }

        mod cas {
            use super::*;

            #[test]
            fn with_matching_expected_target_succeeds() {
                let env = TestEnv::new();
                let target1 = refname!("refs/heads/target1");
                let target2 = refname!("refs/heads/target2");
                let sym_ref = refname!("refs/heads/sym");

                env.repo
                    .write_ref(&target1, Target::Create { target: env.oid1 }, "test")
                    .unwrap();
                env.repo
                    .write_ref(&target2, Target::Create { target: env.oid2 }, "test")
                    .unwrap();

                env.repo
                    .write_symbolic_ref(&sym_ref, SymTarget::create(&target1), "test")
                    .unwrap();

                env.repo
                    .write_symbolic_ref(&sym_ref, SymTarget::cas(&target2, &target1), "test")
                    .unwrap();

                let target = env.repo.ref_target(&sym_ref).unwrap();
                assert_eq!(target, Some(env.oid2));
            }

            #[test]
            fn with_mismatched_expected_target_returns_error() {
                let env = TestEnv::new();
                let target1 = refname!("refs/heads/target1");
                let target2 = refname!("refs/heads/target2");
                let sym_ref = refname!("refs/heads/sym");

                env.repo
                    .write_ref(&target1, Target::Create { target: env.oid1 }, "test")
                    .unwrap();
                env.repo
                    .write_ref(&target2, Target::Create { target: env.oid2 }, "test")
                    .unwrap();

                env.repo
                    .write_symbolic_ref(&sym_ref, SymTarget::create(&target1), "test")
                    .unwrap();

                let err = env
                    .repo
                    .write_symbolic_ref(&sym_ref, SymTarget::cas(&target2, &target2), "test")
                    .unwrap_err();

                assert!(matches!(err, WriteSymbolicRef::CasFailed { .. }));

                let target = env.repo.ref_target(&sym_ref).unwrap();
                assert_eq!(target, Some(env.oid1));
            }
        }

        mod delete {
            use super::*;

            #[test]
            fn with_existing_ref_succeeds() {
                let env = TestEnv::new();
                let target_ref = refname!("refs/heads/target");
                let sym_ref = refname!("refs/heads/sym");

                env.repo
                    .write_ref(&target_ref, Target::Create { target: env.oid1 }, "test")
                    .unwrap();
                env.repo
                    .write_symbolic_ref(&sym_ref, SymTarget::create(&target_ref), "test")
                    .unwrap();

                env.repo.delete_ref(&sym_ref).unwrap();

                let target = env.repo.ref_target(&sym_ref).unwrap();
                assert_eq!(target, None);
            }

            #[test]
            fn with_non_existent_ref_succeeds_idempotently() {
                let env = TestEnv::new();
                let sym_ref = refname!("refs/heads/delete_non_existent");

                env.repo.delete_ref(&sym_ref).unwrap();
            }
        }
    }

    mod list {
        use super::*;
        use crate::git::repository::reference::error::read::ListReference;
        use radicle_git_ref_format::refspec::PatternString;
        use std::fs;

        mod glob_matching {
            use super::*;

            #[test]
            fn yields_only_matching_references() {
                let env = TestEnv::new();

                let ref1 = refname!("refs/heads/a");
                let ref2 = refname!("refs/tags/b");
                let ref3 = refname!("refs/rad/c");

                env.repo
                    .write_ref(&ref1, Target::Create { target: env.oid1 }, "test")
                    .unwrap();
                env.repo
                    .write_ref(&ref2, Target::Create { target: env.oid1 }, "test")
                    .unwrap();
                env.repo
                    .write_ref(&ref3, Target::Create { target: env.oid1 }, "test")
                    .unwrap();

                let pattern = PatternString::try_from("refs/heads/*").unwrap();
                let mut refs: Vec<_> = env.repo.list_refs(&pattern).unwrap().collect();

                assert_eq!(refs.len(), 1);
                let (name, oid) = refs.pop().unwrap().unwrap();
                assert_eq!(name.as_str(), "refs/heads/a");
                assert_eq!(oid, env.oid1);
            }
        }

        mod malformed_refs {
            use super::*;

            #[test]
            fn yields_parse_error_for_invalid_refname() {
                let env = TestEnv::new();

                // Write a file with an invalid reference name
                let bad_ref_path = env.repo.path().join("refs/heads/bad..name");
                std::fs::write(&bad_ref_path, env.oid1.to_string() + "\n").unwrap();

                let pattern = PatternString::try_from("refs/heads/*").unwrap();
                let mut refs: Vec<_> = env.repo.list_refs(&pattern).unwrap().collect();

                assert_eq!(refs.len(), 1);
                let err = refs.pop().unwrap().unwrap_err();
                assert!(matches!(err, ListReference::Parse { .. }));
            }

            #[test]
            fn yields_error_for_bad_ref_but_continues() {
                let env = TestEnv::new();

                let good_ref = refname!("refs/heads/good");
                env.repo
                    .write_ref(&good_ref, Target::Create { target: env.oid1 }, "test")
                    .unwrap();

                // Write a malformed reference directly to the filesystem
                let bad_ref_path = env.repo.path().join("refs/heads/bad");
                fs::write(&bad_ref_path, "ref: refs/heads/bad\n").unwrap();

                let pattern = PatternString::try_from("refs/heads/*").unwrap();
                let mut refs: Vec<_> = env.repo.list_refs(&pattern).unwrap().collect();

                // Sort to ensure deterministic order for assertions
                refs.sort_by_key(|r| r.is_ok());

                assert_eq!(refs.len(), 2);

                let err = refs[0].as_ref().unwrap_err();
                assert!(matches!(err, ListReference::Peel { .. }));

                let (name, oid) = refs[1].as_ref().unwrap();
                assert_eq!(name.as_str(), "refs/heads/good");
                assert_eq!(*oid, env.oid1);
            }
        }
    }
}
