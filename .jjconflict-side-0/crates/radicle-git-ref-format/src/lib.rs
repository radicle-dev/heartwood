#![no_std]

//! [`git_ref_format`]: https://crates.io/crates/git-ref-format
//! [`radicle-git-ext`]: https://crates.io/crates/radicle-git-ext
//!
//! This crate depends on and re-exports from [`git_ref_format_core`].
//!
//! ## Macros
//!
//! Instead of providing procedural macros, like [`git_ref_format`]
//! it just provides much simpler declarative macros, guarded by the feature
//! flag `macro`.
//!
//! ### Benefits
//!
//! - Does not depend on [`radicle-git-ext`].
//! - Does not pull in procedural macro dependencies.
//! - Has much smaller compile-time overhead than [`git_ref_format`].
//!
//! ### Drawback
//!
//! The main drawback is that the macros in this crate cannot provide compile
//! time validation of the argument. Thus, these macros must be used in
//! conjunction with testing: If all generated objects are used in tests, and
//! these tests are run, then the guarantees are equally strong. Consumers that
//! do not or cannot test their code should not use the macros then.

pub use git_ref_format_core::*;

/// Create a [`git_ref_format_core::RefString`] from a string literal.
///
/// Similar to [`core::debug_assert`], an optimized build will not validate
/// (but rather perform an unsafe conversion) unless `-C debug-assertions` is
/// passed to the compiler.
#[cfg(any(feature = "macro", test))]
#[macro_export]
macro_rules! refname {
    ($arg:literal) => {{
        use $crate::RefString;

        #[cfg(debug_assertions)]
        {
            RefString::try_from($arg).expect(core::concat!(
                "literal `",
                $arg,
                "` must be a valid reference name"
            ))
        }

        #[cfg(not(debug_assertions))]
        {
            extern crate alloc;

            use alloc::string::String;

            let s: String = $arg.to_owned();
            unsafe { core::mem::transmute::<_, RefString>(s) }
        }
    }};
}

/// Create a [`git_ref_format_core::Qualified`] from a string literal.
///
/// Similar to [`core::debug_assert`], an optimized build will not validate
/// (but rather perform an unsafe conversion) unless `-C debug-assertions` is
/// passed to the compiler.
#[cfg(any(feature = "macro", test))]
#[macro_export]
macro_rules! qualified {
    ($arg:literal) => {{
        use $crate::Qualified;

        #[cfg(debug_assertions)]
        {
            Qualified::from_refstr($crate::refname!($arg)).expect(core::concat!(
                "literal `",
                $arg,
                "` must be of the form 'refs/<category>/<name>'"
            ))
        }

        #[cfg(not(debug_assertions))]
        {
            extern crate alloc;

            use core::mem::transmute;

            use alloc::borrow::Cow;
            use alloc::string::String;

            use $crate::{RefStr, RefString};

            let s: String = $arg.to_owned();
            let refstring: RefString = unsafe { transmute(s) };
            let cow: Cow<'_, RefStr> = Cow::Owned(refstring);
            let qualified: Qualified = unsafe { transmute(cow) };

            qualified
        }
    }};
}

/// Create a [`git_ref_format_core::Component`] from a string literal.
///
/// Similar to [`core::debug_assert`], an optimized build will not validate
/// (but rather perform an unsafe conversion) unless `-C debug-assertions` is
/// passed to the compiler.
#[cfg(any(feature = "macro", test))]
#[macro_export]
macro_rules! component {
    ($arg:literal) => {{
        use $crate::Component;

        #[cfg(debug_assertions)]
        {
            Component::from_refstr($crate::refname!($arg)).expect(core::concat!(
                "literal `",
                $arg,
                "` must be a valid component (cannot contain '/')"
            ))
        }

        #[cfg(not(debug_assertions))]
        {
            extern crate alloc;

            use core::mem::transmute;

            use alloc::borrow::Cow;
            use alloc::string::String;

            use $crate::{RefStr, RefString};

            let s: String = $arg.to_owned();
            let refstring: RefString = unsafe { transmute(s) };
            let cow: Cow<'_, RefStr> = Cow::Owned(refstring);
            let component: Component = unsafe { transmute(cow) };

            component
        }
    }};
}

/// Create a [`git_ref_format_core::refspec::PatternString`] from a string literal.
///
/// Similar to [`core::debug_assert`], an optimized build will not validate
/// (but rather perform an unsafe conversion) unless `-C debug-assertions` is
/// passed to the compiler.
#[cfg(any(feature = "macro", test))]
#[macro_export]
macro_rules! pattern {
    ($arg:literal) => {{
        use $crate::refspec::PatternString;

        #[cfg(debug_assertions)]
        {
            PatternString::try_from($arg).expect(core::concat!(
                "literal `",
                $arg,
                "` must be a valid refspec pattern"
            ))
        }

        #[cfg(not(debug_assertions))]
        {
            extern crate alloc;

            use alloc::string::String;

            let s: String = $arg.to_owned();
            unsafe { core::mem::transmute::<_, PatternString>(s) }
        }
    }};
}

/// Create a [`git_ref_format_core::refspec::QualifiedPattern`] from a string literal.
///
/// Similar to [`core::debug_assert`], an optimized build will not validate
/// (but rather perform an unsafe conversion) unless `-C debug-assertions` is
/// passed to the compiler.
#[cfg(any(feature = "macro", test))]
#[macro_export]
macro_rules! qualified_pattern {
    ($arg:literal) => {{
        use $crate::refspec::QualifiedPattern;

        #[cfg(debug_assertions)]
        {
            use core::concat;

            use $crate::refspec::PatternStr;

            let pattern = PatternStr::try_from_str($arg).expect(concat!(
                "literal `",
                $arg,
                "` must be a valid refspec pattern"
            ));

            QualifiedPattern::from_patternstr(pattern).expect(concat!(
                "literal `",
                $arg,
                "` must be a valid qualified refspec pattern"
            ))
        }

        #[cfg(not(debug_assertions))]
        {
            extern crate alloc;

            use core::mem::transmute;

            use alloc::borrow::Cow;
            use alloc::string::String;

            use $crate::refspec::{PatternStr, PatternString};

            let s: String = $arg.to_owned();
            let pattern: PatternString = unsafe { transmute(s) };
            let cow: Cow<'_, PatternStr> = Cow::Owned(pattern);
            let qualified: QualifiedPattern = unsafe { transmute(cow) };

            qualified
        }
    }};
}

#[cfg(test)]
mod test {
    #[test]
    fn refname() {
        let _ = crate::refname!("refs/heads/main");
        let _ = crate::refname!("refs/tags/v1.0.0");
        let _ = crate::refname!("refs/remotes/origin/main");
        let _ = crate::refname!("a");
    }

    #[test]
    #[should_panic]
    fn refname_invalid() {
        let _ = crate::refname!("a~b");
    }

    #[test]
    fn qualified() {
        let _ = crate::qualified!("refs/heads/main");
        let _ = crate::qualified!("refs/tags/v1.0.0");
        let _ = crate::qualified!("refs/remotes/origin/main");
    }

    #[test]
    #[should_panic]
    fn qualified_invalid() {
        let _ = crate::qualified!("a");
    }

    #[test]
    fn component() {
        let _ = crate::component!("a");
    }

    #[test]
    #[should_panic]
    fn component_invalid() {
        let _ = crate::component!("a/b");
    }

    #[test]
    fn pattern() {
        let _ = crate::pattern!("refs/heads/main");
        let _ = crate::pattern!("refs/tags/v1.0.0");
        let _ = crate::pattern!("refs/remotes/origin/main");

        let _ = crate::pattern!("a");
        let _ = crate::pattern!("a/*");
        let _ = crate::pattern!("*");
        let _ = crate::pattern!("a/b*");
        let _ = crate::pattern!("a/b*/c");
        let _ = crate::pattern!("a/*/c");
    }

    #[test]
    fn qualified_pattern() {
        let _ = crate::qualified_pattern!("refs/heads/main");
        let _ = crate::qualified_pattern!("refs/tags/v1.0.0");
        let _ = crate::qualified_pattern!("refs/remotes/origin/main");

        let _ = crate::qualified_pattern!("refs/heads/main/*");
        let _ = crate::qualified_pattern!("refs/tags/v*");
        let _ = crate::qualified_pattern!("refs/remotes/origin/main");
        let _ = crate::qualified_pattern!("refs/remotes/origin/department/*/person");
    }

    #[test]
    #[should_panic]
    fn qualified_pattern_invalid() {
        let _ = crate::qualified_pattern!("a/*/b");
    }
}
