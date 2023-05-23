use std::path::Path;
use std::{fmt, io};

use radicle::git;
use radicle_surf::diff::{FileDiff, Hunk, Modification};
use radicle_term::ansi::Style;

use crate::terminal as term;

/// Writes git-compatible diffs to the given stream.
#[derive(Default)]
pub struct DiffWriter<W: io::Write> {
    styled: bool,
    stream: W,
}

impl<W: io::Write> DiffWriter<W> {
    /// Create a new writer from a writable stream.
    pub fn new(stream: W) -> Self {
        Self {
            stream,
            styled: false,
        }
    }

    /// Consume writer and return the underlying stream.
    pub fn into_inner(self) -> W {
        self.stream
    }

    /// Use styled output or not.
    pub fn styled(mut self, choice: bool) -> Self {
        self.styled = choice;
        self
    }

    /// Write a diff file header.
    pub fn file_header(&mut self, file: &FileDiff) -> io::Result<()> {
        fn diff(old: &Path, new: &Path) -> String {
            format!("diff --git a/{} b/{}", old.display(), new.display())
        }

        match file {
            FileDiff::Modified(f) => {
                self.meta(diff(&f.path, &f.path))?;
                self.meta(format!(
                    "index {}..{} {:o}",
                    term::format::oid(f.old.oid),
                    term::format::oid(f.new.oid),
                    u32::from(f.new.mode.clone())
                ))?;
                self.meta(format!("--- a/{}", f.path.display()))?;
                self.meta(format!("+++ b/{}", f.path.display()))?;
            }
            FileDiff::Added(f) => {
                self.meta(diff(&f.path, &f.path))?;
                self.meta(format!("new file mode {:o}", u32::from(f.new.mode.clone())))?;
                self.meta(format!(
                    "index {}..{}",
                    term::format::oid(git::raw::Oid::zero()),
                    term::format::oid(*f.new.oid),
                ))?;
                self.meta("--- /dev/null")?;
                self.meta(format!("+++ b/{}", f.path.display()))?;
            }
            FileDiff::Copied(_) => todo!(),
            FileDiff::Deleted(f) => {
                self.meta(diff(&f.path, &f.path))?;
                self.meta(format!(
                    "deleted file mode {:o}",
                    u32::from(f.old.mode.clone())
                ))?;
                self.meta(format!(
                    "index {}..{}",
                    term::format::oid(*f.old.oid),
                    term::format::oid(git::raw::Oid::zero()),
                ))?;
                self.meta(format!("--- a/{}", f.path.display()))?;
                self.meta("+++ /dev/null")?;
            }
            FileDiff::Moved(f) => {
                self.meta(diff(&f.old_path, &f.new_path))?;
                // Nb. We only display diffs as moves when the file was not changed.
                self.meta("similarity index 100%")?;
                self.meta(format!("rename from {}", f.old_path.display()))?;
                self.meta(format!("rename to {}", f.new_path.display()))?;
            }
        };

        Ok(())
    }

    /// Write a diff hunk.
    pub fn hunk(&mut self, hunk: &Hunk<Modification>) -> io::Result<()> {
        self.magenta(hunk.header.from_utf8_lossy().trim_end())?;

        for modification in &hunk.lines {
            match modification {
                Modification::Deletion(radicle_surf::diff::Deletion { line, .. }) => {
                    self.deleted(format!(
                        "-{}",
                        String::from_utf8_lossy(line.as_bytes()).trim_end()
                    ))?;
                }
                Modification::Addition(radicle_surf::diff::Addition { line, .. }) => {
                    self.added(format!("+{}", line.from_utf8_lossy()).trim_end())?;
                }
                Modification::Context { line, .. } => {
                    self.context(format!(" {}", line.from_utf8_lossy().trim_end()))?;
                }
            }
        }
        Ok(())
    }

    fn write(&mut self, s: impl fmt::Display, style: Style) -> io::Result<()> {
        if self.styled {
            writeln!(self.stream, "{}", term::Paint::new(s).with_style(style))
        } else {
            writeln!(self.stream, "{s}")
        }
    }

    fn meta(&mut self, s: impl fmt::Display) -> io::Result<()> {
        self.write(s, term::Style::new(term::Color::Yellow))
    }

    fn magenta(&mut self, s: impl fmt::Display) -> io::Result<()> {
        self.write(s, term::Style::new(term::Color::Magenta))
    }

    fn deleted(&mut self, s: impl fmt::Display) -> io::Result<()> {
        self.write(s, term::Style::new(term::Color::Red))
    }

    fn added(&mut self, s: impl fmt::Display) -> io::Result<()> {
        self.write(s, term::Style::new(term::Color::Green))
    }

    fn context(&mut self, s: impl fmt::Display) -> io::Result<()> {
        self.write(s, term::Style::default().dim())
    }
}
