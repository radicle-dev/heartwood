use std::{ffi::OsString, process};

pub fn run(args: Vec<OsString>) -> anyhow::Result<()> {
    crate::warning::deprecated("rad diff", "git diff");

    let mut child = process::Command::new("git")
        .arg("diff")
        .args(args)
        .spawn()?;

    let exit_status = child.wait()?;

    process::exit(exit_status.code().unwrap_or(1));
}
