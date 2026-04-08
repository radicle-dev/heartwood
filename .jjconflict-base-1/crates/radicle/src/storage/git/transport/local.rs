//! The local git transport protocol.
pub mod url;
pub use url::{Url, UrlError};

use std::cell::RefCell;
use std::process;
use std::str::FromStr;
use std::sync::Once;

use crate::git;
use crate::storage;
use crate::storage::git::Storage;

use super::ChildStream;

thread_local! {
    /// Stores a storage instance per thread.
    /// This avoids race conditions when used in a multi-threaded context.
    static THREAD_STORAGE: RefCell<Option<Storage>> = RefCell::default();
}

/// Local git transport over the filesystem.
#[derive(Default)]
struct Local {
    /// The child process we spawn.
    child: RefCell<Option<process::Child>>,
}

impl crate::git::raw::transport::SmartSubtransport for Local {
    fn action(
        &self,
        url: &str,
        service: git::raw::transport::Service,
    ) -> Result<Box<dyn git::raw::transport::SmartSubtransportStream>, git::raw::Error> {
        let url =
            Url::from_str(url).map_err(|e| git::raw::Error::from_str(e.to_string().as_str()))?;
        let service: &str = match service {
            git::raw::transport::Service::UploadPack
            | git::raw::transport::Service::UploadPackLs => "upload-pack",
            git::raw::transport::Service::ReceivePack
            | git::raw::transport::Service::ReceivePackLs => "receive-pack",
        };
        let git_dir = THREAD_STORAGE
            .with(|t| {
                t.borrow()
                    .as_ref()
                    .map(|s| storage::git::paths::repository(&s, &url.repo))
            })
            .ok_or_else(|| {
                git::raw::Error::from_str("local transport storage was not registered")
            })?;

        let mut cmd = process::Command::new("git");

        if let Some(ns) = url.namespace {
            cmd.env("GIT_NAMESPACE", ns.to_string());
        }

        let mut child = cmd
            .arg(service)
            .arg(&git_dir)
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped())
            .stderr(process::Stdio::inherit())
            .spawn()
            .map_err(|e| git::raw::Error::from_str(e.to_string().as_str()))?;

        let stdin = child.stdin.take().expect("taking stdin is safe");
        let stdout = child.stdout.take().expect("taking stdout is safe");

        self.child.replace(Some(child));

        Ok(Box::new(ChildStream { stdout, stdin }))
    }

    fn close(&self) -> Result<(), git::raw::Error> {
        if let Some(mut child) = self.child.take() {
            let result = child
                .wait()
                .map_err(|e| git::raw::Error::from_str(e.to_string().as_str()))?;

            if !result.success() {
                return if let Some(code) = result.code() {
                    Err(git::raw::Error::from_str(
                        format!("transport: child process exited with error code {code}").as_str(),
                    ))
                } else {
                    Err(git::raw::Error::from_str(
                        "transport: child process exited with unknown error",
                    ))
                };
            }
        }
        Ok(())
    }
}

// TODO: Instead of taking a storage here, we should take something that can return a storage path.
/// Register a storage with the local transport protocol.
pub fn register(storage: Storage) {
    static REGISTER: Once = Once::new();

    THREAD_STORAGE.with(|s| {
        *s.borrow_mut() = Some(storage);
    });

    REGISTER.call_once(|| unsafe {
        git::raw::transport::register(Url::SCHEME, move |remote| {
            git::raw::transport::Transport::smart(remote, false, Local::default())
        })
        .expect("local transport registration");
    });
}
