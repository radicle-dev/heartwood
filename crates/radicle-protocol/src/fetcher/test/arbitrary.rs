use std::collections::HashSet;

use radicle::{identity::DocAt, test::arbitrary};

use crate::fetcher::{commands, Command, Fetched};

impl qcheck::Arbitrary for Fetched {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        Fetched {
            updated: vec![],
            namespaces: HashSet::arbitrary(g),
            clone: bool::arbitrary(g),
            doc: DocAt::arbitrary(g),
        }
    }
}

impl qcheck::Arbitrary for Command {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        todo!()
    }
}

impl qcheck::Arbitrary for commands::Fetch {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        todo!()
    }
}

impl qcheck::Arbitrary for commands::Fetched {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        g.choose(&[
            commands::Fetched::DequeueFetches,
            commands::Fetched::Fetched {
                from: arbitrary::gen(g.size()),
                rid: arbitrary::gen(g.size()),
            },
        ])
        .cloned()
        .unwrap()
    }
}

impl qcheck::Arbitrary for commands::Dequeue {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        g.choose(&[commands::Dequeue::Nodes {
            nodes: arbitrary::gen(5),
        }])
        .cloned()
        .unwrap()
    }
}
