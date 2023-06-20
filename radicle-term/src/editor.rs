use std::ffi::OsString;
use std::io::IsTerminal;
use std::io::Write;
use std::os::fd::{AsRawFd, FromRawFd};
use std::path::PathBuf;
use std::process;
use std::{env, fs, io};

pub const COMMENT_FILE: &str = "RAD_COMMENT";

/// Allows for text input in the configured editor.
pub struct Editor {
    path: PathBuf,
}

impl Drop for Editor {
    fn drop(&mut self) {
        fs::remove_file(&self.path).ok();
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

impl Editor {
    /// Create a new editor.
    pub fn new() -> Self {
        let path = env::temp_dir().join(COMMENT_FILE);

        Self { path }
    }

    /// Set the file extension.
    pub fn extension(mut self, ext: &str) -> Self {
        let ext = ext.trim_start_matches('.');

        self.path.set_extension(ext);
        self
    }

    /// Open the editor and return the edited text.
    ///
    /// If the text hasn't changed from the initial contents of the editor,
    /// return `None`.
    pub fn edit(&mut self, initial: impl ToString) -> io::Result<Option<String>> {
        let initial = initial.to_string();
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(&self.path)?;

        if file.metadata()?.len() == 0 {
            file.write_all(initial.as_bytes())?;
            if !initial.ends_with('\n') {
                file.write_all(b"\n")?;
            }
            file.flush()?;
        }

        let Some(cmd) = self::default_editor() else {
            return Err(
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "editor not configured: the `EDITOR` environment variable is not set"
                )
            );
        };

        // We duplicate the stderr file descriptor to pass it to the child process, otherwise, if
        // we simply pass the `RawFd` of our stderr, `Command` will close our stderr when the
        // child exits.
        let stderr = io::stderr().as_raw_fd();
        let stderr = unsafe { libc::dup(stderr) };
        let stdin = if io::stdin().is_terminal() {
            process::Stdio::inherit()
        } else {
            let tty = termion::get_tty()?;
            // If standard input is not a terminal device, the editor won't work correctly.
            // In that case, we use the terminal device, eg. `/dev/tty` as standard input.
            process::Stdio::from(tty)
        };

        process::Command::new(cmd)
            .stdout(unsafe { process::Stdio::from_raw_fd(stderr) })
            .stderr(process::Stdio::inherit())
            .stdin(stdin)
            .arg(&self.path)
            .spawn()?
            .wait()?;

        let text = fs::read_to_string(&self.path)?;
        if text.trim().is_empty() {
            return Ok(None);
        }
        Ok(Some(text))
    }
}

/// Get the default editor command.
pub fn default_editor() -> Option<OsString> {
    if let Ok(visual) = env::var("VISUAL") {
        if !visual.is_empty() {
            return Some(visual.into());
        }
    }
    if let Ok(editor) = env::var("EDITOR") {
        if !editor.is_empty() {
            return Some(editor.into());
        }
    }
    None
}
