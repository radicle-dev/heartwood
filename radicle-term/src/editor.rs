use std::ffi::OsString;
use std::io::IsTerminal;
use std::io::Write;
use std::os::fd::{AsRawFd, FromRawFd};
use std::path::{Path, PathBuf};
use std::process;
use std::{env, fs, io};

pub const COMMENT_FILE: &str = "RAD_COMMENT";
/// Some common paths where system-installed binaries are found.
pub const PATHS: &[&str] = &["/usr/local/bin", "/usr/bin", "/bin"];

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
    pub fn edit(&mut self, initial: impl AsRef<[u8]>) -> io::Result<Option<String>> {
        let initial = initial.as_ref();
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.path)?;

        if file.metadata()?.len() == 0 {
            file.write_all(initial)?;
            if !initial.ends_with(&[b'\n']) {
                file.write_all(b"\n")?;
            }
            file.flush()?;
        }

        let Some(cmd) = self::default_editor() else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "editor not configured: the `EDITOR` environment variable is not set",
            ));
        };
        let Some(parts) = shlex::split(cmd.to_string_lossy().as_ref()) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid editor command {cmd:?}"),
            ));
        };
        let Some((program, args)) = parts.split_first() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid editor command {cmd:?}"),
            ));
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

        process::Command::new(program)
            .stdout(unsafe { process::Stdio::from_raw_fd(stderr) })
            .stderr(process::Stdio::inherit())
            .stdin(stdin)
            .args(args)
            .arg(&self.path)
            .spawn()
            .map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!("failed to spawn editor command {cmd:?}: {e}"),
                )
            })?
            .wait()
            .map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!("editor command {cmd:?} didn't spawn: {e}"),
                )
            })?;

        let text = fs::read_to_string(&self.path)?;
        if text.trim().is_empty() {
            return Ok(None);
        }
        Ok(Some(text))
    }
}

/// Get the default editor command.
pub fn default_editor() -> Option<OsString> {
    // First check the standard environment variables.
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
    // Check Git. The user might have configured their editor there.
    #[cfg(feature = "git2")]
    if let Ok(path) = git2::Config::open_default().and_then(|cfg| cfg.get_path("core.editor")) {
        return Some(path.into_os_string());
    }
    // On macOS, `nano` is installed by default and it's what most users are used to
    // in the terminal.
    if cfg!(target_os = "macos") && exists("nano") {
        return Some("nano".into());
    }
    // If all else fails, we try `vi`. It's usually installed on most unix-based systems.
    if exists("vi") {
        return Some("vi".into());
    }
    None
}

/// Check whether a binary can be found in the most common paths.
/// We don't bother checking the $PATH variable, as we're only looking for very standard tools
/// and prefer not to make this too complex.
fn exists(cmd: &str) -> bool {
    for dir in PATHS {
        if Path::new(dir).join(cmd).exists() {
            return true;
        }
    }
    false
}
