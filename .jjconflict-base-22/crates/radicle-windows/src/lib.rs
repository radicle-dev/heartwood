//! Library for interaction with Windows, specialized for Radicle.

#[cfg(windows)]
pub mod jobs;

#[cfg(windows)]
pub mod process {
    pub mod creation_flags {
        pub use windows::Win32::System::Threading::{
            CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, DETACHED_PROCESS,
        };
    }
}
