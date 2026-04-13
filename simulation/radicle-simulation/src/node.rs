//! Node and Repository abstractions for the Radicle simulation environment.
//!
//! This module provides the core types used to interact with Radicle nodes
//! running inside the Kubernetes cluster. It includes executors for running
//! commands, persona builders for identity management, and high-level wrappers
//! for Radicle CLI operations (issues, patches, syncing).

use std::process::Command;
use std::str;
use std::thread;
use std::time::Duration;
use uuid::Uuid;

/// Escapes single quotes for safe interpolation inside a shell single-quoted string.
fn escape_sh(s: &str) -> String {
    s.replace('\'', "'\\''")
}

/// Trait defining how commands are executed within a Radicle node's environment.
pub trait NodeExecutor {
    /// Executes a command on the specified node and returns the combined stdout/stderr.
    fn exec(&self, node_name: &str, cmd: &[&str]) -> Result<String, String>;
}

/// Default executor implementation that uses the `kubectl exec` CLI command.
///
/// This executor shells out to the local `kubectl` binary to run commands inside
/// the `node` container of the target Kubernetes pod.
#[derive(Clone, Debug)]
pub struct KubectlExecutor;

impl NodeExecutor for KubectlExecutor {
    fn exec(&self, node_name: &str, cmd: &[&str]) -> Result<String, String> {
        let mut args = vec!["exec", "-i", node_name, "-c", "node", "--"];
        args.extend_from_slice(cmd);

        let output = Command::new("kubectl")
            .args(&args)
            .output()
            .map_err(|e| format!("Failed to execute kubectl: {}", e))?;

        let stdout = str::from_utf8(&output.stdout)
            .unwrap_or("")
            .trim()
            .to_string();
        let stderr = str::from_utf8(&output.stderr)
            .unwrap_or("")
            .trim()
            .to_string();

        if output.status.success() {
            // Git commands often write success messages to stderr, so we combine them
            let mut combined = stdout;
            if !stderr.is_empty() {
                combined.push('\n');
                combined.push_str(&stderr);
            }
            Ok(combined.trim().to_string())
        } else {
            let mut combined = stderr;
            if !stdout.is_empty() {
                combined.push('\n');
                combined.push_str(&stdout);
            }
            Err(format!(
                "Command failed: {}\nStderr: {}",
                args.join(" "),
                combined.trim()
            ))
        }
    }
}

/// Represents a Radicle node running in the simulated network.
///
/// This struct provides methods to interact with the node's CLI, manage its Git
/// identity (persona), and initialize or clone repositories.
#[derive(Clone, Debug)]
pub struct Node<E = KubectlExecutor> {
    /// Kubernetes pod name of the node.
    pub name: String,
    /// The executor used to run commands on this node.
    pub executor: E,
    /// Optional Git identity (Name, Email) assigned to this node.
    pub persona: Option<(String, String)>,
}

impl Node<KubectlExecutor> {
    /// Creates a new `Node` instance using the default `KubectlExecutor`.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            executor: KubectlExecutor,
            persona: None,
        }
    }
}

impl<E: NodeExecutor + Clone> Node<E> {
    /// Internal helper to set the Git identity on the node and store it in the struct.
    pub fn with_persona(mut self, name: &str, email: &str) -> Result<Self, String> {
        self.setup_identity(name, email)?;
        self.persona = Some((name.to_string(), email.to_string()));
        Ok(self)
    }

    /// Configures the node with the standard "Alice" test persona.
    pub fn as_alice(self) -> Result<Self, String> {
        self.with_persona("Alice", "alice@radicle.local")
    }

    /// Executes a command inside the node container via the configured executor.
    ///
    /// This method automatically logs the command being executed (at the `INFO` level)
    /// and its output or failure (at the `DEBUG` or `ERROR` levels).
    pub fn exec(&self, cmd: &[&str]) -> Result<String, String> {
        let identity = if let Some((name, _)) = &self.persona {
            format!("{}@{}", name, self.name)
        } else {
            self.name.clone()
        };

        log::info!("[{}] $ {}", identity, cmd.join(" "));

        let result = self.executor.exec(&self.name, cmd);

        match &result {
            Ok(output) if !output.trim().is_empty() => {
                log::debug!("[{}] Output:\n{}", identity, output.trim());
            }
            Err(err) => {
                log::error!("[{}] Failed:\n{}", identity, err);
            }
            _ => {}
        }

        result
    }

    /// Convenience wrapper for executing raw shell scripts inside the node.
    pub fn exec_sh(&self, script: &str) -> Result<String, String> {
        self.exec(&["sh", "-c", script])
    }

    /// Configures the global Git user name and email on the node.
    ///
    /// Uses a retry loop to prevent parallel tests from failing due to `gitconfig` file locks.
    pub fn setup_identity(&self, name: &str, email: &str) -> Result<(), String> {
        let name_esc = escape_sh(name);
        let email_esc = escape_sh(email);

        let mut retries = 10;
        while retries > 0 {
            if self
                .exec_sh(&format!("git config --global user.name '{name_esc}'"))
                .is_ok()
            {
                break;
            }
            retries -= 1;
            thread::sleep(Duration::from_millis(100));
        }
        if retries == 0 {
            return Err("Failed to set git config user.name".to_string());
        }

        let mut retries = 10;
        while retries > 0 {
            if self
                .exec_sh(&format!("git config --global user.email '{email_esc}'"))
                .is_ok()
            {
                break;
            }
            retries -= 1;
            thread::sleep(Duration::from_millis(100));
        }
        if retries == 0 {
            return Err("Failed to set git config user.email".to_string());
        }

        Ok(())
    }

    /// Initializes a new Radicle repository on this node.
    ///
    /// This method automatically appends a UUID to the repository name to ensure
    /// parallel tests do not collide. It initializes a Git repository, creates the
    /// specified number of commits, and initializes it as a Radicle project.
    pub fn init_test_repo(
        &self,
        base_name: &str,
        desc: &str,
        commits: u32,
    ) -> Result<Repository<E>, String> {
        let (author, email) = self
            .persona
            .as_ref()
            .ok_or("Persona not set! Call .as_alice() (or similar) before creating a repo.")?;

        let uuid = Uuid::new_v4()
            .to_string()
            .chars()
            .take(6)
            .collect::<String>();
        let repo_name = format!("{}-{}-{}", base_name, author, uuid);

        let author_esc = escape_sh(author);
        let email_esc = escape_sh(email);
        let desc_esc = escape_sh(desc);

        let script = format!(
            "mkdir -p {repo_name} && \
            cd {repo_name} && \
            git init && \
            git config user.name '{author_esc}' && \
            git config user.email '{email_esc}' && \
            echo 'commit 1' > file_1.txt && \
            git add file_1.txt && \
            git commit -m 'Commit 1' && \
            rad init --name {repo_name} --description '{desc_esc}' --public --no-confirm"
        );
        self.exec_sh(&script)?;

        if commits > 1 {
            for i in 2..=commits {
                let commit_script = format!(
                    "cd {repo_name} && \
                    echo 'commit {i}' > file_{i}.txt && \
                    git add file_{i}.txt && \
                    git commit -m 'Commit {i}' && \
                    git push rad master"
                );
                self.exec_sh(&commit_script)?;
            }
        }

        let rid = self.exec_sh(&format!("cd {} && rad . 2>/dev/null", repo_name))?;

        Ok(Repository {
            node: self.clone(),
            name: repo_name,
            rid,
        })
    }

    /// Waits until the Radicle node API is fully responsive.
    pub fn wait_until_responsive(&self) -> Result<(), String> {
        let mut retries = 30;
        while retries > 0 {
            if self.exec(&["rad", "node", "status"]).is_ok() {
                return Ok(());
            }
            retries -= 1;
            thread::sleep(Duration::from_secs(1));
        }
        Err(format!("Node {} did not become responsive", self.name))
    }
}

/// Represents a Radicle repository residing on a specific node.
///
/// This struct provides ergonomic, high-level methods for interacting with
/// the repository, such as syncing, managing issues, and handling patches.
/// All commands executed through this struct are automatically run within
/// the repository's directory on the node.
#[derive(Clone, Debug)]
pub struct Repository<E = KubectlExecutor> {
    /// The node where this repository resides.
    pub node: Node<E>,
    /// The local directory name of the repository.
    pub name: String,
    /// The Radicle ID (RID) of the repository.
    pub rid: String,
}
