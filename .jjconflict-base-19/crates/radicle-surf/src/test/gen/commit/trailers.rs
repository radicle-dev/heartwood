use proptest::{collection, strategy::Strategy};
use radicle_git_metadata::commit::trailers::{OwnedTrailer, Token, Trailer};

use crate::test::r#gen;

pub fn trailers(n: usize) -> impl Strategy<Value = Vec<OwnedTrailer>> {
    collection::vec(trailer(), 0..n)
}

pub fn trailer() -> impl Strategy<Value = OwnedTrailer> {
    (r#gen::alpha(), r#gen::alphanumeric()).prop_map(|(token, value)| {
        Trailer {
            token: Token::try_from(format!("X-{}", token).as_str()).unwrap(),
            value: value.into(),
        }
        .to_owned()
    })
}
