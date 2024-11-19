use std::{cell::Cell, ops::Add};

thread_local! {
    /// The constant time used by the stable-commit-ids feature.
    pub static STABLE_TIME: Cell<i64> = const { Cell::new(1514817556) };
    /// An incrementing counter to advance the `STABLE_TIME` value with in
    /// [`with_advanced_timestamp`].
    pub static STEP: Cell<Step> = Cell::new(Step::default());
}

#[derive(Clone, Copy)]
struct Step(i64);

impl Default for Step {
    fn default() -> Self {
        Self(1)
    }
}

impl Add<Step> for i64 {
    type Output = i64;

    fn add(self, rhs: Step) -> Self::Output {
        self + rhs.0
    }
}

impl Add<i64> for Step {
    type Output = Step;

    fn add(self, rhs: i64) -> Self::Output {
        Step(self.0 + rhs)
    }
}

/// Read the current value of `STABLE_TIME`.
///
/// # Panics
///
/// The `STABLE_TIME` is declared in `thread_local`, and so the panic
/// information is repeated here.
///
/// Panics if the key currently has its destructor running, and it may panic if
/// the destructor has previously been run for this thread.
#[allow(clippy::unwrap_used)]
pub fn read_timestamp() -> i64 {
    STABLE_TIME.get()
}

/// Perform an action `f` that would rely on the `STABLE_TIME` value. This will
/// advance the `STABLE_TIME` by an increment of `1` for each time it is called,
/// within the same thread.
///
/// # Usage
///
/// ```rust, ignore
/// let oid1 = with_advanced_timestamp(|| cob.update("New revision OID"));
/// let oid2 = with_advanced_timestamp(|| cob.update("Another revision OID"));
/// ```
///
/// # Panics
///
/// The `STABLE_TIME` is declared in `thread_local`, and so the panic
/// information is repeated here.
///
/// Panics if the key currently has its destructor running, and it may panic if
/// the destructor has previously been run for this thread.
#[allow(clippy::unwrap_used)]
pub fn with_advanced_timestamp<F, T>(f: F) -> T
where
    F: FnOnce() -> T,
{
    let step = STEP.get();
    let original = read_timestamp();
    STABLE_TIME.replace(original + step);
    let result = f();
    STEP.replace(step + 1);
    result
}
