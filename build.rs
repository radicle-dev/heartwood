use std::env;
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set a build-time `GIT_HEAD` env var which includes the commit id;
    // such that we can tell which code is running.
    let hash = env::var("GIT_HEAD").unwrap_or_else(|_| {
        Command::new("git")
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
            .unwrap_or("unknown".into())
    });

    let version = if let Ok(version) = env::var("RADICLE_VERSION") {
        version
    } else {
        "pre-release".to_owned()
    };

    // Set a build-time `GIT_COMMIT_TIME` env var which includes the commit time.
    let commit_time = env::var("GIT_COMMIT_TIME").unwrap_or_else(|_| {
        Command::new("git")
            .arg("log")
            .arg("-1")
            .arg("--pretty=%ct")
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
            .unwrap_or(0.to_string())
    });

    println!("cargo::rustc-env=RADICLE_VERSION={version}");
    println!("cargo::rustc-env=GIT_COMMIT_TIME={commit_time}");
    println!("cargo::rustc-env=GIT_HEAD={hash}");

    Ok(())
}
