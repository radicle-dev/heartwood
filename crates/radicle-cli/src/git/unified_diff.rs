//! Formatting support for Git's [diff format](https://git-scm.com/docs/diff-format).
use std::fmt;
use std::io;
use std::path::PathBuf;

use thiserror::Error;

use radicle::git;
use radicle_surf::diff::{Diff, DiffContent, DiffFile, FileDiff, Hunk, Modification};

use crate::terminal as term;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
}

/// The kind of FileDiff Header which can be used to print the FileDiff information which precedes
/// `Hunks`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FileHeader {
    Added {
        path: PathBuf,
        new: DiffFile,
        binary: bool,
    },
    Copied {
        old_path: PathBuf,
        new_path: PathBuf,
    },
    Deleted {
        path: PathBuf,
        old: DiffFile,
        binary: bool,
    },
    Modified {
        path: PathBuf,
        old: DiffFile,
        new: DiffFile,
        binary: bool,
    },
    Moved {
        old_path: PathBuf,
        new_path: PathBuf,
    },
}

impl std::convert::From<&FileDiff> for FileHeader {
    // TODO: Pathnames with 'unusual names' need to be quoted.
    fn from(value: &FileDiff) -> Self {
        match value {
            FileDiff::Modified(v) => FileHeader::Modified {
                path: v.path.clone(),
                old: v.old.clone(),
                new: v.new.clone(),
                binary: matches!(v.diff, DiffContent::Binary),
            },
            FileDiff::Added(v) => FileHeader::Added {
                path: v.path.clone(),
                new: v.new.clone(),
                binary: matches!(v.diff, DiffContent::Binary),
            },
            FileDiff::Copied(c) => FileHeader::Copied {
                old_path: c.old_path.clone(),
                new_path: c.new_path.clone(),
            },
            FileDiff::Deleted(v) => FileHeader::Deleted {
                path: v.path.clone(),
                old: v.old.clone(),
                binary: matches!(v.diff, DiffContent::Binary),
            },
            FileDiff::Moved(v) => FileHeader::Moved {
                old_path: v.old_path.clone(),
                new_path: v.new_path.clone(),
            },
        }
    }
}

/// Diff-related types that can be encoded intro the unified diff format.
pub trait Encode: Sized {
    /// Encode type into diff writer.
    fn encode(&self, w: &mut Writer) -> Result<(), Error>;

    /// Encode into unified diff string.
    fn to_unified_string(&self) -> Result<String, Error> {
        let mut buf = Vec::new();
        let mut w = Writer::new(&mut buf);

        w.encode(self)?;
        drop(w);

        String::from_utf8(buf).map_err(Error::from)
    }
}

impl Encode for Diff {
    fn encode(&self, w: &mut Writer) -> Result<(), Error> {
        for fdiff in self.files() {
            fdiff.encode(w)?;
        }
        Ok(())
    }
}

impl Encode for DiffContent {
    fn encode(&self, w: &mut Writer) -> Result<(), Error> {
        match self {
            DiffContent::Plain { hunks, .. } => {
                for h in hunks.iter() {
                    h.encode(w)?;
                }
            }
            DiffContent::Empty => {}
            DiffContent::Binary => todo!("DiffContent::Binary encoding not implemented"),
        }
        Ok(())
    }
}

impl Encode for FileDiff {
    fn encode(&self, w: &mut Writer) -> Result<(), Error> {
        w.encode(&FileHeader::from(self))?;
        match self {
            FileDiff::Modified(f) => {
                w.encode(&f.diff)?;
            }
            FileDiff::Added(f) => {
                w.encode(&f.diff)?;
            }
            FileDiff::Copied(f) => {
                w.encode(&f.diff)?;
            }
            FileDiff::Deleted(f) => {
                w.encode(&f.diff)?;
            }
            FileDiff::Moved(f) => {
                // Nb. We only display diffs as moves when the file was not changed.
                w.encode(&f.diff)?;
            }
        }

        Ok(())
    }
}

impl Encode for FileHeader {
    fn encode(&self, w: &mut Writer) -> Result<(), Error> {
        match self {
            FileHeader::Modified { path, old, new, .. } => {
                w.meta(format!(
                    "diff --git a/{} b/{}",
                    path.display(),
                    path.display()
                ))?;

                if old.mode == new.mode {
                    w.meta(format!(
                        "index {}..{} {:o}",
                        term::format::oid(old.oid),
                        term::format::oid(new.oid),
                        u32::from(old.mode.clone()),
                    ))?;
                } else {
                    w.meta(format!("old mode {:o}", u32::from(old.mode.clone())))?;
                    w.meta(format!("new mode {:o}", u32::from(new.mode.clone())))?;
                    w.meta(format!(
                        "index {}..{}",
                        term::format::oid(old.oid),
                        term::format::oid(new.oid)
                    ))?;
                }

                w.meta(format!("--- a/{}", path.display()))?;
                w.meta(format!("+++ b/{}", path.display()))?;
            }
            FileHeader::Added { path, new, .. } => {
                w.meta(format!(
                    "diff --git a/{} b/{}",
                    path.display(),
                    path.display()
                ))?;

                w.meta(format!("new file mode {:o}", u32::from(new.mode.clone())))?;
                w.meta(format!(
                    "index {}..{}",
                    term::format::oid(git::Oid::ZERO_SHA1),
                    term::format::oid(new.oid),
                ))?;

                w.meta("--- /dev/null")?;
                w.meta(format!("+++ b/{}", path.display()))?;
            }
            FileHeader::Copied { .. } => todo!(),
            FileHeader::Deleted { path, old, .. } => {
                w.meta(format!(
                    "diff --git a/{} b/{}",
                    path.display(),
                    path.display()
                ))?;

                w.meta(format!(
                    "deleted file mode {:o}",
                    u32::from(old.mode.clone())
                ))?;
                w.meta(format!(
                    "index {}..{}",
                    term::format::oid(old.oid),
                    term::format::oid(git::Oid::ZERO_SHA1)
                ))?;

                w.meta(format!("--- a/{}", path.display()))?;
                w.meta("+++ /dev/null".to_string())?;
            }
            FileHeader::Moved { old_path, new_path } => {
                w.meta(format!(
                    "diff --git a/{} b/{}",
                    old_path.display(),
                    new_path.display()
                ))?;
                w.meta("similarity index 100%")?;
                w.meta(format!("rename from {}", old_path.display()))?;
                w.meta(format!("rename to {}", new_path.display()))?;
            }
        };
        Ok(())
    }
}

impl Encode for Hunk<Modification> {
    fn encode(&self, w: &mut Writer) -> Result<(), Error> {
        // TODO: Remove trailing newlines accurately.
        // `trim_end()` will destroy diff information if the diff has a trailing whitespace on
        // purpose.
        w.magenta(self.header.from_utf8_lossy().trim_end())?;
        for l in &self.lines {
            l.encode(w)?;
        }

        Ok(())
    }
}

impl Encode for Modification {
    fn encode(&self, w: &mut Writer) -> Result<(), Error> {
        match self {
            Modification::Deletion(radicle_surf::diff::Deletion { line, .. }) => {
                let s = format!("-{}", String::from_utf8_lossy(line.as_bytes()).trim_end());
                w.write(s, term::Style::new(term::Color::Red))?;
            }
            Modification::Addition(radicle_surf::diff::Addition { line, .. }) => {
                let s = format!("+{}", String::from_utf8_lossy(line.as_bytes()).trim_end());
                w.write(s, term::Style::new(term::Color::Green))?;
            }
            Modification::Context { line, .. } => {
                let s = format!(" {}", String::from_utf8_lossy(line.as_bytes()).trim_end());
                w.write(s, term::Style::default().dim())?;
            }
        }

        Ok(())
    }
}

/// An IO Writer with color printing to the terminal.
pub struct Writer<'a> {
    styled: bool,
    stream: Box<dyn io::Write + 'a>,
}

impl<'a> Writer<'a> {
    pub fn new(w: impl io::Write + 'a) -> Self {
        Self {
            styled: false,
            stream: Box::new(w),
        }
    }

    pub fn encode<T: Encode>(&mut self, arg: &T) -> Result<(), Error> {
        arg.encode(self)?;
        Ok(())
    }

    pub fn write(&mut self, s: impl fmt::Display, style: term::Style) -> io::Result<()> {
        #[cfg(windows)]
        const EOL: &str = "\r\n";

        #[cfg(not(windows))]
        const EOL: &str = "\n";

        if self.styled {
            write!(
                self.stream,
                "{}{EOL}",
                term::Paint::new(s).with_style(style)
            )
        } else {
            write!(self.stream, "{s}{EOL}")
        }
    }

    pub fn meta(&mut self, s: impl fmt::Display) -> io::Result<()> {
        self.write(s, term::Style::new(term::Color::Yellow))
    }

    pub fn magenta(&mut self, s: impl fmt::Display) -> io::Result<()> {
        self.write(s, term::Style::new(term::Color::Magenta))
    }
}
