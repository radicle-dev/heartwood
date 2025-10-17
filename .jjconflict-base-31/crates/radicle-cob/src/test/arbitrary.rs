use std::iter;

use qcheck::Arbitrary;

use crate::{object::ObjectId, TypeName};

impl Arbitrary for TypeName {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let mut rng = fastrand::Rng::with_seed(u64::arbitrary(g));
        let mut name: Vec<String> = Vec::new();
        for _ in 0..rng.usize(1..5) {
            let len = rng.usize(1..16);
            name.push(iter::repeat_with(|| rng.alphanumeric()).take(len).collect());
        }
        name.join(".")
            .parse::<TypeName>()
            .expect("TypeName is valid")
    }
}

impl Arbitrary for ObjectId {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        Self::from(oid::Oid::arbitrary(g))
    }
}
