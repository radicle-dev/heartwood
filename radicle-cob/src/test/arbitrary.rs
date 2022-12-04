use std::{iter, marker::PhantomData};

use qcheck::Arbitrary;

use crate::{ObjectId, TypeName};

#[derive(Clone, Debug)]
pub struct Invalid<T> {
    pub value: String,
    _marker: PhantomData<T>,
}

impl Arbitrary for TypeName {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let rng = fastrand::Rng::with_seed(u64::arbitrary(g));
        let mut name: Vec<String> = Vec::new();
        for _ in 0..rng.usize(1..5) {
            name.push(
                iter::repeat_with(|| rng.alphanumeric())
                    .take(rng.usize(1..16))
                    .collect(),
            );
        }
        name.join(".")
            .parse::<TypeName>()
            .expect("TypeName is valid")
    }
}

impl Arbitrary for ObjectId {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let rng = fastrand::Rng::with_seed(u64::arbitrary(g));
        let bytes = iter::repeat_with(|| rng.u8(..))
            .take(20)
            .collect::<Vec<_>>();
        Self::from(git_ext::Oid::try_from(bytes.as_slice()).unwrap())
    }
}

impl Arbitrary for Invalid<ObjectId> {
    fn arbitrary(g: &mut qcheck::Gen) -> Self {
        let rng = fastrand::Rng::with_seed(u64::arbitrary(g));
        let value = iter::repeat_with(|| rng.alphanumeric())
            .take(rng.usize(21..50))
            .collect();
        Invalid {
            value,
            _marker: PhantomData,
        }
    }
}
