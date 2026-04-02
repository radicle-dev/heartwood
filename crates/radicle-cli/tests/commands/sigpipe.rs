//! Test that `rad` exits cleanly when its standard output is closed earlier
//! before all output is written, i.e., in the case of a ["broken pipe"].
//!
//! On Unix-like systems, when writing to a pipe whose read end was closed,
//! the writing process receives `SIGPIPE`. Often times, this signal
//! is not handled by the process, i.e., no signal handler is installed.
//! The default behaviour in this case is to terminate the process with an
//! exit code of 141 (128 + 13). Such lack of a signal handler is also
//! indidcated by `SIG_DFL`.
//!
//! Rust (since 1.62) ignores `SIGPIPE` by default, see [issue #6529].
//! Ignorance in this case means that the signal handler is set to
//! `SIG_IGN`. Instead, writes to broken pipes return an
//! [`std::io::Error`] of kind [`std::io::ErrorKind::BrokenPipe`].
//!
//! The [`println!`] macro panics on errors when writing, thus also in case
//! [`std::io::ErrorKind::BrokenPipe`] is returned.
//!
//! We would like to allow users to pipe output from `rad` into other commands,
//! which may also stop reading early, such as `head -1` or `less`.
//!
//! Tests in this module check that the `rad` binary handles broken pipes gracefully:
//! It should succeed or fail with an exit code 141,
//! and must not panic (or fail with exit code 101, common for panics).
//!
//! For the definitions of `SIGPIPE`, `SIG_DFL`, and `SIG_IGN`, refer to
//! [`signal.h` in POSIX.1-2024].
//!
//! ["broken pipe"]: https://en.wikipedia.org/wiki/Broken_pipe
//! [issue #62569]: https://github.com/rust-lang/rust/issues/62569
//! [`signal.h` in POSIX.1-2024]: https://pubs.opengroup.org/onlinepubs/9799919799.2024edition/basedefs/signal.h.html

/// A panicking process exits with code 101. A process killed by SIGPIPE
/// shows an exit code of 141 (128 + 13). A clean exit is 0.
/// Any of these except 101 is acceptable.
use std::io::Read;
use std::process::{Command, Stdio};

use radicle::profile;

use crate::util::environment::Environment;

#[ignore = "test fails"]
#[test]
fn config() {
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

/// `rad --help` exercises [`println!`] directly (via clap's help rendering),
/// and not just [`radicle_term::Element::print`].
#[test]
fn help() {
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
