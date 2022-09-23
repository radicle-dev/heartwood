use std::cell::RefCell;
use std::rc::Rc;

use crate::{LocalDuration, LocalTime};

/// Seconds since epoch.
pub type Timestamp = u64;

/// Clock with interior mutability.
#[derive(Debug, Clone)]
pub struct RefClock(Rc<RefCell<LocalTime>>);

impl std::ops::Deref for RefClock {
    type Target = Rc<RefCell<LocalTime>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl RefClock {
    /// Elapse time.
    pub fn elapse(&self, duration: LocalDuration) {
        self.borrow_mut().elapse(duration)
    }

    pub fn local_time(&self) -> LocalTime {
        *self.borrow()
    }

    pub fn set(&mut self, time: LocalTime) {
        *self.borrow_mut() = time;
    }

    pub fn timestamp(&self) -> Timestamp {
        self.local_time().as_secs()
    }
}

impl From<LocalTime> for RefClock {
    fn from(other: LocalTime) -> Self {
        Self(Rc::new(RefCell::new(other)))
    }
}
