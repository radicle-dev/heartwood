use std::io::{self, Write};
use std::process::{Command, Stdio};
use std::string::FromUtf8Error;

#[derive(thiserror::Error, Debug)]
pub enum CommandError {
    #[error("command error: not found")]
    NotFound,
    #[error("command error: an internal error occured.")]
    Internal,
    #[error("command error: converting output failed: {0}")]
    Output(#[from] FromUtf8Error),
    #[error("command error: retrieving output failed: {0}")]
    Other(#[from] io::Error),
}

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

pub fn rad_tui<S: AsRef<std::ffi::OsStr>>(
    args: impl IntoIterator<Item = S>,
) -> anyhow::Result<Option<String>, CommandError> {
    match Command::new("rad-tui")
        .stderr(Stdio::piped())
        .args(args)
        .spawn()
    {
        Ok(child) => {
            let output = child.wait_with_output()?;
            let stderr = String::from_utf8(output.stderr)?.trim().to_owned();

            if !output.status.success() {
                return Err(CommandError::Internal);
            }

            Ok((!stderr.is_empty()).then_some(stderr))
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => Err(CommandError::NotFound),
        Err(err) => Err(CommandError::Other(err)),
    }
}
