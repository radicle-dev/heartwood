use std::sync::{Arc, Mutex};

use crate::{LocalDuration, LocalTime};

/// Seconds since epoch.
pub type Timestamp = u64;

/// Clock with interior mutability.
#[derive(Debug, Clone)]
pub struct RefClock(Arc<Mutex<LocalTime>>);

impl std::ops::Deref for RefClock {
    type Target = Arc<Mutex<LocalTime>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl RefClock {
    /// Elapse time.
    pub fn elapse(&self, duration: LocalDuration) {
        self.lock().unwrap().elapse(duration)
    }

    pub fn local_time(&self) -> LocalTime {
        *self.lock().unwrap()
    }

    pub fn set(&mut self, time: LocalTime) {
        *self.lock().unwrap() = time;
    }

    pub fn timestamp(&self) -> Timestamp {
        self.local_time().as_secs()
    }
}

impl From<LocalTime> for RefClock {
    fn from(other: LocalTime) -> Self {
        Self(Arc::new(Mutex::new(other)))
    }
}
