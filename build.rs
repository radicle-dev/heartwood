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

    let version = env::var("RADICLE_VERSION").unwrap_or_else(|_| {
        // If `RADICLE_VERSION` is not set, we still try our best to
        // describe this version by asking git. The result will in many
        // cases be a reference to the last released version, and how
        // many commits we are ahead, plus a short version of the
        // object ID of `HEAD`, e.g. `releases/x.y.z-80-gefe10f95be-dirty`
        // which would mean that we built 80 commits ahead of release
        // x.y.z, with efe10f95be being a unique prefix of the OID of
        // `HEAD`, and the working directory was dirty.
        // If this is a build pointing to a commit that has release tag, this
        // will just return the tag name itelf, e.g. `releases/x.y.z`.
        // If all fails, we just use `hash`, which, in the worst case is
        // still "unknown" (see above) but in most cases will just be
        // the short OID of `HEAD`.
        Command::new("git")
            .arg("describe")
            .arg("--always")
            .arg("--broken")
            .arg("--dirty")
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    String::from_utf8(output.stdout).ok()
                } else {
                    None
                }
            })
            .unwrap_or(hash.clone())
    });

    // Since in the previous step we are likely to almost always end up with
    // a prefix of `releases/`, as this is the scheme we use in this
    // repository, we remove this common prefix, to get nice version numbers.
    let version = if let Some(stripped) = version.strip_prefix("releases/") {
        stripped.to_owned()
    } else {
        version
    };

    // Set a build-time `SOURCE_DATE_EPOCH` env var which includes the commit time.
    let commit_time = env::var("SOURCE_DATE_EPOCH").unwrap_or_else(|_| {
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
    println!("cargo::rustc-env=SOURCE_DATE_EPOCH={commit_time}");
    println!("cargo::rustc-env=GIT_HEAD={hash}");

    Ok(())
}
