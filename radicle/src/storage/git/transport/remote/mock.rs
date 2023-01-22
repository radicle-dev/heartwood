//! Mock git transport used for mocking the remote transport in tests.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Mutex, Once};
use std::{process, thread};

use once_cell::sync::Lazy;

use super::Url;
use crate::storage::git::transport::ChildStream;
use crate::storage::RemoteId;

/// Nodes registered with the mock transport.
static NODES: Lazy<Mutex<HashMap<RemoteId, PathBuf>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// The mock transport.
#[derive(Default)]
struct MockTransport;

impl git2::transport::SmartSubtransport for MockTransport {
    fn action(
        &self,
        url: &str,
        service: git2::transport::Service,
    ) -> Result<Box<dyn git2::transport::SmartSubtransportStream>, git2::Error> {
        let url = Url::from_str(url).map_err(|e| git2::Error::from_str(e.to_string().as_str()))?;
        let nodes = NODES.lock().expect("lock cannot be poisoned");
        let storage = if let Some(storage) = nodes.get(&url.node) {
            match service {
                git2::transport::Service::ReceivePack | git2::transport::Service::ReceivePackLs => {
                    return Err(git2::Error::from_str(
                        "git-receive-pack is not supported with the mock transport",
                    ));
                }
                _ => {}
            }
            storage
        } else {
            return Err(git2::Error::from_str(&format!(
                "node {} was not registered with the mock transport",
                url.node
            )));
        };
        let git_dir = storage.join(url.repo.canonical());
        let mut cmd = process::Command::new("git");
        let mut child = cmd
            .arg("upload-pack")
            .arg("--strict")
            .arg(&git_dir)
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped())
            .stderr(process::Stdio::inherit())
            .spawn()
            .expect("the `git` command is available");

        let stdin = child.stdin.take().expect("stdin is safe to take");
        let stdout = child.stdout.take().expect("stdout is safe to take");

        thread::spawn(move || child.wait());

        Ok(Box::new(ChildStream { stdout, stdin }))
    }

    fn close(&self) -> Result<(), git2::Error> {
        Ok(())
    }
}

/// Register a new node with the given storage path.
pub fn register(node: &RemoteId, path: &Path) {
    static REGISTER: Once = Once::new();

    REGISTER.call_once(|| unsafe {
        git2::transport::register(Url::SCHEME, move |remote| {
            git2::transport::Transport::smart(remote, false, MockTransport::default())
        })
        .expect("transport registration is successful");
    });

    NODES
        .lock()
        .expect("the lock isn't poisoned")
        .insert(*node, path.to_owned());
}
