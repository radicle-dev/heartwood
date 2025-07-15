use std::env;
use std::ffi::OsString;
use std::path::Path;

use inquire::InquireError;
use thiserror::Error;

/// Allows for text input in the configured editor.
pub struct Editor<'a>(pub inquire::Editor<'a>);

#[derive(Debug, Error)]
#[error(transparent)]
pub struct Error(#[from] InquireError);

impl Editor<'_> {
    /// Create a new editor for editing a comment.
    pub fn comment() -> Self {
        Self::new("Enter comment.", "You may enter your comment using your editor. Pressing (e) will open the editor. Save the file and exit the editor to submit your comment.")
    }

    /// Open the editor and return the edited text.
    ///
    /// If the text hasn't changed from the initial contents of the editor,
    /// return `None`.
    pub fn edit(self) -> Result<Option<String>, Error> {
        match self.0.prompt() {
            Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => Ok(None),
            Err(e) => Err(Error::from(e)),
            Ok(s) => Ok(Some(s)),
        }
    }
}

impl<'a> Editor<'a> {
    /// Create a new editor.
    pub fn new(message: &'a str, help_message: &'a str) -> Self {
        Self(inquire::Editor::new(message).with_help_message(help_message))
    }

    /// Set the file extension.
    pub fn extension(self, ext: &'a str) -> Self {
        debug_assert!(
            ext.starts_with('.'),
            "File extension should start with a dot."
        );
        Self(self.0.with_file_extension(ext))
    }

    /// Initialize the file with the provided `content`, as long as the file
    /// does not already contain anything.
    pub fn initial(self, content: &'a str) -> Self {
        Self(self.0.with_predefined_text(content))
    }

    pub fn editor(self, editor: Option<&'a OsString>) -> Self {
        match editor {
            Some(editor_command) => Self(self.0.with_editor_command(editor_command)),
            None => self,
        }
    }
}

/// Get the default editor command.
pub fn default_editor_command() -> Option<OsString> {
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
    // Some common paths where system-installed binaries are found.
    const PATHS: &[&str] = &["/usr/local/bin", "/usr/bin", "/bin"];

    for dir in PATHS {
        if Path::new(dir).join(cmd).exists() {
            return true;
        }
    }
    false
}
