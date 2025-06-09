use std::io::Write;
use std::process::{Command, Stdio};

pub fn bat<S: AsRef<std::ffi::OsStr>>(
    args: impl IntoIterator<Item = S>,
    stdin: &str,
) -> anyhow::Result<()> {
    let mut child = Command::new("bat")
        .stdin(Stdio::piped())
        .args(args)
        .spawn()?;

    let writer = child.stdin.as_mut().unwrap();
    writer.write_all(stdin.as_bytes())?;

    child.wait()?;

    Ok(())
}
