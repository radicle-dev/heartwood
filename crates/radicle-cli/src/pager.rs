use std::io;
use std::process::{Command, Stdio};

use radicle_term::element;
use radicle_term::{Constraint, Element};

use crate::terminal;

/// Output the given element through a pager, if necessary.
/// If it fits within the screen, don't run it through a pager.
pub fn run(elem: impl Element) -> io::Result<()> {
    let Some(constraint) = Constraint::from_env() else {
        return elem.write(Constraint::UNBOUNDED);
    };
    let Some(rows) = terminal::rows() else {
        return elem.write(Constraint::UNBOUNDED);
    };
    if elem.size(Constraint::UNBOUNDED).rows <= rows {
        return elem.write(Constraint::UNBOUNDED);
    }
    let Some(pager) = radicle::profile::env::pager() else {
        return elem.write(Constraint::UNBOUNDED);
    };
    let Some(parts) = shlex::split(&pager) else {
        return elem.write(Constraint::UNBOUNDED);
    };
    let Some((program, args)) = parts.split_first() else {
        return elem.write(Constraint::UNBOUNDED);
    };

    let mut child = Command::new(program)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .args(args)
        .spawn()?;

    let writer = child.stdin.as_mut().unwrap();
    let result = element::write_to(&elem, writer, constraint);

    child.wait()?;

    match result {
        // This error is expected when the pager is exited.
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => {}
        Err(e) => return Err(e),
        Ok(_) => {}
    }

    Ok(())
}
