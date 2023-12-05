use std::process::{Command, Stdio};

use anyhow::{anyhow, Error};

use crate::git::Rev;
use crate::terminal as term;

pub fn patch_select() -> Result<Option<Rev>, Error> {
    match Command::new("rad")
        .arg("tui")
        .arg("patch")
        .arg("select")
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(tui) => match tui.wait_with_output() {
            Ok(output) => {
                let status = output.status;
                let stderr = String::from_utf8(output.stderr)?;

                if status.success() {
                    if !stderr.is_empty() {
                        Ok(Some(Rev::from(stderr.trim().to_owned())))
                    } else {
                        Err(anyhow!("No patch selected in `rad tui`"))
                    }
                } else {
                    Err(anyhow!("An internal error occured in `rad tui`: {stderr}"))
                }
            }
            Err(err) => Err(anyhow!("Failed to wait on `rad tui`: {err}")),
        },
        Err(_) => {
            term::tip!(
                "An optional patch selector can be enabled by installing `rad-tui`. You can download it from https://files.radicle.xyz/latest.",
            );
            Ok(None)
        }
    }
}
