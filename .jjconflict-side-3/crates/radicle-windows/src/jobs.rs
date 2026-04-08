use std::io;
use std::os::windows::io::AsRawHandle as _;

use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::JobObjects::{AssignProcessToJobObject, CreateJobObjectW, TerminateJobObject},
    },
    core::PCWSTR,
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

/// Wraps a handle to a job object.
/// See also <https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects>.
#[derive(Debug)]
#[repr(transparent)]
pub struct Job {
    handle: HANDLE,
}

impl Job {
    /// Create a new, empty, job object.
    /// See also <https://learn.microsoft.com/windows/win32/api/jobapi2/nf-jobapi2-createjobobjectw>.
    pub fn new() -> Result<Self, Error> {
        #[allow(non_snake_case)]
        // To match the naming in the documentation of `CreateJobObjectW` for clarity.
        let lpJobAttributes = None;

        #[allow(non_snake_case)]
        // To match the naming in the documentation of `CreateJobObjectW` for clarity.
        let lpName = PCWSTR::null();

        // SAFETY: We argue that calling `CrateJobObjectW` is safe based on
        // its documentation (see documentation of this function):
        // Both parameters take their `null`/empty value.
        // For `lpJobAttributes`, this is immediate as we use `None`, and the
        // wrapper in the `windows` crate ensures safety.
        // For `lpName`, we pass `name`, which was initialized to
        // `PCWSTR::null()` (see above) which we rely on to obtain a sane
        // `null` value.
        // For the case of `null`/empty arguments, the documentation of
        // `CreateJobObjectW` does not specify any further preconditions.
        let result = unsafe { CreateJobObjectW(lpJobAttributes, lpName) };

        match result {
            Ok(handle) => Ok(Self { handle }),
            Err(e) => Err(Error::Create(e.into())),
        }
    }

    /// Assign a process to the job object.
    /// See also <https://docs.microsoft.com/windows/win32/api/jobapi2/nf-jobapi2-assignprocesstojobobject>.
    pub fn assign(&self, child: &std::process::Child) -> Result<(), Error> {
        #[allow(non_snake_case)]
        // To match the naming in the documentation of `AssignProcessToJobObject` for clarity.
        let hJob = self.handle;

        #[allow(non_snake_case)]
        // To match the naming in the documentation of `AssignProcessToJobObject` for clarity.
        let hProcess = HANDLE(child.as_raw_handle());

        // SAFETY: We argue that calling `AssignProcessToJobObject` is safe based on
        // its documentation (see documentation of this function):
        // First, we argue that the argument
        // For `hJob`, consider that its value is the same as `self.handle` (see above),
        // and that `self.handle` is only assigned in `Self::new` via a call to `CreateJobObjectW`.
        // For `hProcess`, we pass the raw handle of the child process we were given.
        // Thus, we rely on `impl std::os::windows::io::AsRawHandle for std::process::Child`
        // for its value to be a valid handle to a process.
        // Note that the documentation of `AssignProcessToJobObject` specifies further preconditions
        // on the arguments, especially concerning:
        //  - Job Object Security (see <https://learn.microsoft.com/windows/win32/procthread/job-object-security-and-access-rights>)
        //  - Process Security and Access Rights (see <https://learn.microsoft.com/windows/win32/procthread/process-security-and-access-rights>)
        // We assume that violations of these preconditions will be reflected in
        // the return value of `AssignProcessToJobObject`, and will not result in safety violations.
        let result = unsafe { AssignProcessToJobObject(hJob, hProcess) };

        result.map_err(|e| Error::Assign(e.into()))
    }

    /// Terminate a job object.
    /// See also <https://learn.microsoft.com/windows/win32/api/jobapi2/nf-jobapi2-terminatejobobject>.
    pub fn terminate(self, exit_code: u32) -> Result<(), Error> {
        #[allow(non_snake_case)]
        // To match the naming in the documentation of `TerminateJobObject` for clarity.
        let hJob = self.handle;

        #[allow(non_snake_case)]
        // To match the naming in the documentation of `TerminateJobObject` for clarity.
        let uExitCode = exit_code;

        // SAFETY: We argue that calling `TerminateJobObject` is safe based on
        // its documentation (see documentation of this function):
        // For `hJob`, consider that its value is the same as `self.handle`,
        // and that `self.handle` is only assigned in `Self::new` via a call to `CreateJobObjectW`.
        // For `uExitCode`, there are no preconditions to satisfy.
        // Note that the documentation of `TerminateJobObject` specifies further preconditions
        // on the arguments, especially concerning:
        //  - Job Object Security (see <https://learn.microsoft.com/windows/win32/procthread/job-object-security-and-access-rights>)
        //  - Process Security and Access Rights (see <https://learn.microsoft.com/windows/win32/procthread/process-security-and-access-rights>)
        // We assume that violations of these preconditions will be reflected in
        // the return value of `AssignProcessToJobObject`, and will not result in safety violations.
        let result = unsafe { TerminateJobObject(hJob, uExitCode) };

        result.map_err(|e| Error::Terminate(e.into()))
    }

    /// Convenience method to create a new job and assign a child process to it.
    /// See also [`Job::new`] and [`Job::assign`].
    pub fn for_child(child: &std::process::Child) -> Result<Self, Error> {
        let job = Self::new()?;
        job.assign(child)?;
        Ok(job)
    }
}

impl Drop for Job {
    /// Close the handle to the job object.
    /// See also <https://learn.microsoft.com/windows/win32/api/handleapi/nf-handleapi-closehandle>.
    fn drop(&mut self) {
        #[allow(non_snake_case)]
        // To match the naming in the documentation of `CloseHandle` for clarity.
        let hObject = self.handle;

        // SAFETY: We argue that calling `CloseHandle` is safe based on
        // its documentation (see documentation of this function):
        // For `hObject`, consider that its value is the same as `self.handle`,
        // and that `self.handle` is only assigned in `Self::new` via a call to `CreateJobObjectW`,
        // thus is a valid handle to a job object.
        let _ = unsafe { CloseHandle(hObject) };
    }
}
