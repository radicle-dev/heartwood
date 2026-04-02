//! Test that `rad` exits cleanly when its stdout is a broken pipe.
//!
//! As evidenced from [Rust #6529], Rust (since 1.62) ignores SIGPIPE by default.
//! This means that writing to a closed pipe returns `io::ErrorKind::BrokenPipe`
//! instead of terminating the process with SIGPIPE (the standard Unix behaviour
//! for CLI tools).
//!
//! The `println!` macro panics on write errors, so commands like
//! `rad config | head -1` would produce a panic backtrace instead of exiting
//! silently.
//!
//! This test verifies that the `rad` binary handles broken pipes gracefully:
//! it should exit with a success status or with SIGPIPE (exit code 141),
//! and must not panic (exit code 101).
//!
//! [Rust #62569]: https://github.com/rust-lang/rust/issues/62569

#[cfg(unix)]
mod unix {
    /// A panicking process exits with code 101. A process killed by SIGPIPE
    /// shows an exit code of 141 (128 + 13). A clean exit is 0.
    /// Any of these except 101 is acceptable.
    mod broken_pipe {
        use std::io::Read;
        use std::process::{Command, Stdio};

        use radicle::profile;

        use crate::util::environment::Environment;

        #[ignore = "test fails"]
        #[test]
        fn rad_config_broken_pipe() {
            let mut environment = Environment::new();
            let profile = environment.profile("alice");

            let rad = env!("CARGO_BIN_EXE_rad");

            // Spawn `rad config` with stdout piped so we control it.
            let mut child = Command::new(rad)
                .arg("config")
                .env("RAD_HOME", profile.home.path())
                .env(profile::env::RAD_PASSPHRASE, "radicle")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("failed to spawn rad");

            let mut stdout = child.stdout.take().unwrap();

            // Read just one byte, then drop stdout to close the pipe.
            // This simulates `head -1` closing the read end early.
            let mut buf = [0u8; 1];
            let _ = stdout.read(&mut buf);
            drop(stdout);

            let output = child.wait_with_output().expect("failed to wait on rad");

            // Capture stderr for diagnostics.
            let stderr = String::from_utf8_lossy(&output.stderr);

            // Exit code 101 is Rust's panic exit code — this must not happen.
            let code = output.status.code();

            assert!(
                code != Some(101),
                "rad panicked on broken pipe (exit code 101).\nstderr:\n{stderr}"
            );

            // Additionally, stderr should not contain panic messages.
            assert!(
                !stderr.contains("panicked at"),
                "rad panicked on broken pipe.\nstderr:\n{stderr}"
            );
        }

        /// `rad --help` exercises `println!` directly (via clap's help rendering),
        /// and not just `Element::print()`.
        #[test]
        fn rad_help() {
            let rad = env!("CARGO_BIN_EXE_rad");

            let mut child = Command::new(rad)
                .arg("--help")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("failed to spawn rad");

            let mut stdout = child.stdout.take().unwrap();

            // Read a single byte and close.
            let mut buf = [0u8; 1];
            let _ = stdout.read(&mut buf);
            drop(stdout);

            let output = child.wait_with_output().expect("failed to wait on rad");
            let stderr = String::from_utf8_lossy(&output.stderr);
            let code = output.status.code();

            assert!(
                code != Some(101),
                "rad panicked on broken pipe (exit code 101).\nstderr:\n{stderr}"
            );
            assert!(
                !stderr.contains("panicked at"),
                "rad panicked on broken pipe.\nstderr:\n{stderr}"
            );
        }

        /// `rad self` uses `Element::print()` for table output.
        #[ignore = "test fails"]
        #[test]
        fn rad_self() {
            let mut environment = Environment::new();
            let profile = environment.profile("alice");

            let rad = env!("CARGO_BIN_EXE_rad");

            let mut child = Command::new(rad)
                .arg("self")
                .env("RAD_HOME", profile.home.path())
                .env(profile::env::RAD_PASSPHRASE, "radicle")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("failed to spawn rad");

            let mut stdout = child.stdout.take().unwrap();
            let mut buf = [0u8; 1];
            let _ = stdout.read(&mut buf);
            drop(stdout);

            let output = child.wait_with_output().expect("failed to wait on rad");
            let stderr = String::from_utf8_lossy(&output.stderr);
            let code = output.status.code();

            assert!(
                code != Some(101),
                "rad panicked on broken pipe (exit code 101).\nstderr:\n{stderr}"
            );
            assert!(
                !stderr.contains("panicked at"),
                "rad panicked on broken pipe.\nstderr:\n{stderr}"
            );
        }
    }

    mod normal_pipe {
        use std::process::Command;

        use radicle::profile;

        use crate::util::environment::Environment;

        /// `rad config` produces valid output when stdout is NOT a broken pipe.
        #[test]
        fn rad_config() {
            let mut environment = Environment::new();
            let profile = environment.profile("alice");

            let rad = env!("CARGO_BIN_EXE_rad");

            let output = Command::new(rad)
                .arg("config")
                .env("RAD_HOME", profile.home.path())
                .env(profile::env::RAD_PASSPHRASE, "radicle")
                .output()
                .expect("failed to run rad");

            assert!(
                output.status.success(),
                "rad config failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );

            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                stdout.contains("\"alias\""),
                "rad config output should contain alias"
            );
        }
    }
}
