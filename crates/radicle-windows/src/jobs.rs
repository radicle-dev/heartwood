use std::io;
use std::os::windows::io::AsRawHandle as _;

use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::JobObjects::{AssignProcessToJobObject, CreateJobObjectW, TerminateJobObject},
    },
};

use thiserror::Error;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("Failed to create job: {0}")]
    Create(io::Error),
    #[error("Failed to assign job: {0}")]
    Assign(io::Error),
    #[error("Failed to terminate job: {0}")]
    Terminate(io::Error),
}

impl From<Error> for io::Error {
    fn from(value: Error) -> Self {
        use Error::*;
        match value {
            Create(error) | Assign(error) | Terminate(error) => error,
        }
    }
}

#[derive(Debug)]
#[repr(transparent)]
pub struct Job {
    pub(crate) handle: HANDLE,
}

unsafe impl Send for Job {}
unsafe impl Sync for Job {}

impl Job {
    pub fn new() -> Result<Self, Error> {
        unsafe { CreateJobObjectW(None, PCWSTR::null()) }
            .map_err(|e| Error::Create(e.into()))
            .map(|handle| Self { handle })
    }

    /// Assign a process to the job object.
    /// See also <https://docs.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-assignprocesstojobobject>.
    pub fn assign(&self, child: &std::process::Child) -> Result<(), Error> {
        let handle = child.as_raw_handle();
        unsafe { AssignProcessToJobObject(self.handle, HANDLE(handle)) }
            .map_err(|e| Error::Assign(e.into()))
    }

    pub fn terminate(self, exit_code: u32) -> Result<(), Error> {
        unsafe { TerminateJobObject(self.handle, exit_code) }
            .map_err(|e| Error::Terminate(e.into()))
    }

    /// Convenience method to create a job and assign a child process to it.
    pub fn for_child(child: &std::process::Child) -> Result<Self, Error> {
        let job = Self::new()?;
        job.assign(child)?;
        Ok(job)
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.handle) };
    }
}
