pub(crate) mod commit;

use proptest::prelude::*;

/// Any unicode "word" is trivially a valid refname.
#[cfg(feature = "serde")]
pub fn trivial() -> impl Strategy<Value = String> {
    "\\w+"
}

#[cfg(feature = "serde")]
pub fn valid() -> impl Strategy<Value = String> {
    prop::collection::vec(trivial(), 1..20).prop_map(|xs| xs.join("/"))
}

pub fn alphanumeric() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9_]+"
}

pub fn alpha() -> impl Strategy<Value = String> {
    "[a-zA-Z]+"
}
