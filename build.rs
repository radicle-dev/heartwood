use std::process::Command;
use std::env;

fn main() {
    // Set a build-time `GIT_HEAD` env var which includes the commit id;
    // such that we can tell which code is running.
    let hash = Command::new("git")
        .arg("rev-parse")
        .arg("--short")
        .arg("HEAD")
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        })
        .unwrap_or(env::var("GIT_HEAD").unwrap_or("unknown".into()));

    println!("cargo:rustc-env=GIT_HEAD={hash}");
    println!("cargo:rustc-rerun-if-changed=.git/HEAD");
}
