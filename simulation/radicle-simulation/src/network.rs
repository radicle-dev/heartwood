//! Network lifecycle management for the Radicle simulation environment.

use crate::constants::*;
use crate::node::Node;
use std::io::Write;
use std::process::{Command, Stdio};
use std::str;
use std::sync::{Arc, Mutex, Weak};

/// A guard that ensures the network is torn down when tests finish.
pub struct NetworkGuard {
    cue_path: String,
}

impl Drop for NetworkGuard {
    fn drop(&mut self) {
        if std::env::var("PRESERVE_NETWORK").is_ok() {
            println!(
                "⏭️ PRESERVE_NETWORK is set. Skipping cleanup for {}.",
                self.cue_path
            );
            return;
        }

        println!("🔄 Tearing down network topology from {}...", self.cue_path);
        let status = Command::new("timoni")
            .args(["delete", TIMONI_INSTANCE_NAME])
            .status();

        match status {
            Ok(s) if s.success() => println!("✅ Network torn down successfully."),
            _ => eprintln!("⚠️ Warning: Failed to cleanly tear down network."),
        }
    }
}

static NETWORK_GUARD: Mutex<Weak<NetworkGuard>> = Mutex::new(Weak::new());

/// Applies the specified CUE network topology to the Kubernetes cluster.
///
/// This function evaluates the CUE files, pipes the resulting JSON to Timoni,
/// and waits for all pods and Radicle nodes to become fully responsive.
pub fn apply_network(cue_path: &str) -> Arc<NetworkGuard> {
    let mut weak_guard = NETWORK_GUARD.lock().unwrap();

    // If the network is already running, just return a clone of the Arc
    if let Some(arc) = weak_guard.upgrade() {
        return arc;
    }

    println!("🔄 Applying network topology from {}...", cue_path);

    // Merge and evaluate CUE files into a single JSON stream
    let cue_export = Command::new("cue")
        .args(["export", SCHEMA_CUE_PATH, cue_path, "--out", "json"])
        .output()
        .expect("Failed to execute cue export");

    if !cue_export.status.success() {
        panic!(
            "CUE export failed:\n{}",
            String::from_utf8_lossy(&cue_export.stderr)
        );
    }

    // Pipe the evaluated JSON directly into Timoni
    let mut timoni = Command::new("timoni")
        .args([
            "apply",
            TIMONI_INSTANCE_NAME,
            TIMONI_MODULE_PATH,
            "--values",
            "-", // The '-' tells Timoni to read values from stdin
        ])
        .stdin(Stdio::piped())
        .spawn()
        .expect("Failed to spawn timoni");

    // Write the JSON to Timoni's stdin
    if let Some(mut stdin) = timoni.stdin.take() {
        stdin
            .write_all(&cue_export.stdout)
            .expect("Failed to write to timoni stdin");
    }

    let status = timoni.wait().expect("Failed to wait on timoni");
    assert!(status.success(), "Timoni apply failed for {}", cue_path);

    // Wait for pods to be Ready
    println!("🔄 Waiting for pods to be ready...");
    let wait_status = Command::new("kubectl")
        .args([
            "wait",
            "--for=condition=Ready",
            "pod",
            "-l",
            "app=radicle-node",
            "--timeout=300s",
        ])
        .status()
        .expect("Failed to wait for pods");
    assert!(wait_status.success(), "Pods did not become ready in time");

    // Wait for Radicle nodes to be fully responsive
    println!("🔄 Waiting for Radicle nodes to be fully responsive...");
    let output = Command::new("kubectl")
        .args([
            "get",
            "pods",
            "-l",
            "app=radicle-node",
            "-o",
            "jsonpath={.items[*].metadata.name}",
        ])
        .output()
        .expect("Failed to execute kubectl get pods");

    let stdout = str::from_utf8(&output.stdout).unwrap_or("");
    let pods: Vec<&str> = stdout.split_whitespace().collect();

    for pod_name in pods {
        let node = Node::new(pod_name);
        node.wait_until_responsive()
            .expect("Node did not become responsive");
    }
    println!("✅ Network {} is ready!", cue_path);

    let arc = Arc::new(NetworkGuard {
        cue_path: cue_path.to_string(),
    });
    *weak_guard = Arc::downgrade(&arc);

    arc
}
