use qcheck_macros::quickcheck;

use crate::fetcher::QueuedFetch;

#[quickcheck]
fn reflexive(item: QueuedFetch) -> bool {
    item == item.clone()
}

#[quickcheck]
fn symmetric(a: QueuedFetch, b: QueuedFetch) -> bool {
    (a == b) == (b == a)
}

#[quickcheck]
fn transitive(a: QueuedFetch, b: QueuedFetch, c: QueuedFetch) -> bool {
    if a == b && b == c {
        a == c
    } else {
        true
    }
}
