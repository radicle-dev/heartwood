use std::ffi::OsString;

use anyhow::anyhow;
use radicle::Profile;
use radicle_term as term;

#[derive(PartialEq, Eq)]
pub enum Operation {
    Exec { query: String },
}

pub fn db(profile: &Profile, args: Vec<OsString>) -> anyhow::Result<()> {
    use lexopt::prelude::*;

    let mut parser = lexopt::Parser::from_args(args);
    let mut op: Option<Operation> = None;

    while let Some(arg) = parser.next()? {
        match arg {
            Value(cmd) if op.is_none() => match cmd.to_string_lossy().as_ref() {
                "exec" => {
                    let val = parser
                        .value()
                        .map_err(|_| anyhow!("a query to execute must be provided for `exec`"))?;
                    op = Some(Operation::Exec {
                        query: val.to_string_lossy().to_string(),
                    });
                }
                unknown => anyhow::bail!("unknown operation '{unknown}'"),
            },
            _ => return Err(anyhow!(arg.unexpected())),
        }
    }

    match op.ok_or_else(|| anyhow!("a command must be provided, eg. `rad node db exec`"))? {
        Operation::Exec { query } => {
            let db = profile.database_mut()?;
            db.execute(query)?;

            let changed = db.change_count();
            if changed > 0 {
                term::success!("{changed} row(s) affected.");
            } else {
                term::print(term::format::italic("No rows affected."));
            }
        }
    }
    Ok(())
}
