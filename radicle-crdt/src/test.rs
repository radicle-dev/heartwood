use std::fmt::Debug;
use std::rc::Rc;

use super::*;

/// Generate test values following a weight distribution.
pub struct WeightedGenerator<'a, T, C> {
    cases: Vec<Rc<dyn Fn(&mut C, fastrand::Rng) -> Option<T> + 'a>>,
    rng: fastrand::Rng,
    ctx: C,
}

impl<'a, T, C> Iterator for WeightedGenerator<'a, T, C> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let cases = self.cases.len();

        loop {
            let r = self.rng.usize(0..cases);
            let g = &self.cases[r];

            if let Some(val) = g(&mut self.ctx, self.rng.clone()) {
                return Some(val);
            }
        }
    }
}

impl<'a, T, C: Default> WeightedGenerator<'a, T, C> {
    /// Create a new distribution.
    pub fn new(rng: fastrand::Rng) -> Self {
        Self {
            cases: Vec::new(),
            rng,
            ctx: C::default(),
        }
    }

    /// Add a new variant with a given weight and generator function.
    pub fn variant(
        mut self,
        weight: usize,
        generator: impl Fn(&mut C, fastrand::Rng) -> Option<T> + 'a,
    ) -> Self {
        let gen = Rc::new(generator);
        for _ in 0..weight {
            self.cases.push(gen.clone());
        }
        self
    }
}

/// Assert semilattice ACI laws.
pub fn assert_laws<S: Debug + Semilattice + PartialEq + Clone>(a: &S, b: &S, c: &S) {
    assert_associative(a, b, c);
    assert_commutative(a, b);
    assert_commutative(b, c);
    assert_idempotent(a);
    assert_idempotent(b);
    assert_idempotent(c);
}

pub fn assert_associative<S: Debug + Semilattice + PartialEq + Clone>(a: &S, b: &S, c: &S) {
    // (a ^ b) ^ c
    let s1 = a.clone().join(b.clone()).join(c.clone());
    // a ^ (b ^ c)
    let s2 = a.clone().join(b.clone().join(c.clone()));
    // (a ^ b) ^ c = a ^ (b ^ c)
    assert_eq!(s1, s2, "associativity");
}

pub fn assert_commutative<S: Debug + Semilattice + PartialEq + Clone>(a: &S, b: &S) {
    // a ^ b
    let s1 = a.clone().join(b.clone());
    // b ^ a
    let s2 = b.clone().join(a.clone());
    // a ^ b = b ^ a
    assert_eq!(s1, s2, "commutativity");
}

pub fn assert_idempotent<S: Debug + Semilattice + PartialEq + Clone>(a: &S) {
    // a ^ a
    let s1 = a.clone().join(a.clone());
    // a
    let s2 = a.clone();
    // a ^ a = a
    assert_eq!(s1, s2, "idempotence");
}

#[test]
fn test_generator() {
    let rng = fastrand::Rng::with_seed(0);
    let dist = WeightedGenerator::<char, ()>::new(rng)
        .variant(1, |_, _| Some('a'))
        .variant(2, |_, _| Some('b'))
        .variant(4, |_, _| Some('c'))
        .variant(8, |_, _| Some('d'));

    let values = dist.take(1000).collect::<Vec<_>>();

    let a = values.iter().filter(|c| **c == 'a').count();
    let b = values.iter().filter(|c| **c == 'b').count();
    let c = values.iter().filter(|c| **c == 'c').count();
    let d = values.iter().filter(|c| **c == 'd').count();

    assert_eq!(a, 63);
    assert_eq!(b, 151);
    assert_eq!(c, 255);
    assert_eq!(d, 531);
}
