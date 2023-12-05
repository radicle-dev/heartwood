use std::process::{Command, Stdio};

use anyhow::{anyhow, Error};

use crate::git::Rev;
use crate::terminal as term;

pub fn run_patch_selector() -> Result<Option<Rev>, Error> {
    match Command::new("rad-tui")
        .arg("patch")
        .arg("list")
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(tui) => match tui.wait_with_output() {
            Ok(output) => {
                let stderr = String::from_utf8(output.stderr)?;
                let rev = Rev::from(stderr.trim().to_owned());

                Ok(Some(rev))
            }
            Err(err) => Err(anyhow!("an interal error occured: {err}")),
        },
        Err(_) => {
            term::tip!(
                "An optional patch selector can be enabled by installing 'rad-tui'. You can download it from https://files.radicle.xyz/latest.",
            );
            Ok(None)
        }
    }
}
