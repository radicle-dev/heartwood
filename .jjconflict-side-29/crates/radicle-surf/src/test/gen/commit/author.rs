use proptest::strategy::{Just, Strategy};
use radicle_git_metadata::author::{Author, Time};

use crate::test::r#gen;

pub fn author() -> impl Strategy<Value = Author> {
    r#gen::alphanumeric().prop_flat_map(move |name| {
        (Just(name), r#gen::alphanumeric()).prop_flat_map(|(name, domain)| {
            (Just(name), Just(domain), (0..1000i64)).prop_map(move |(name, domain, time)| {
                let email = format!("{name}@{domain}");
                Author {
                    name,
                    email,
                    time: Time::new(time, 0),
                }
            })
        })
    })
}
