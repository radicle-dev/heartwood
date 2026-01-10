use std::str::FromStr;

use crate::{Author, Commit, Time};
use proptest::prelude::*;
use radicle_oid::Oid;

proptest! {
    #[test]
    fn prop_test_commits(commit in commits_strategy()) {
        super::roundtrip::json(commit)
    }
}

fn commits_strategy() -> impl Strategy<Value = Commit> {
    ("[a-fA-F0-9]{40}", any::<String>(), any::<i64>()).prop_map(|(id, text, time)| Commit {
        id: Oid::from_str(&id).unwrap(),
        author: Author {
            name: text.clone(),
            email: text.clone(),
            time: Time::new(time, 0),
        },
        committer: Author {
            name: text.clone(),
            email: text.clone(),
            time: Time::new(time, 0),
        },
        message: text.clone(),
        summary: text,
        parents: vec![Oid::from_str(&id).unwrap(), Oid::from_str(&id).unwrap()],
    })
}
