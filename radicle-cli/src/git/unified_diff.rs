//! Formatting support for Git's [diff format](https://git-scm.com/docs/diff-format).
use std::fmt;
use std::io;
use std::path::PathBuf;

use crate::terminal as term;

use radicle::git::raw::Oid;
use radicle_surf::diff::{Diff, DiffContent, DiffFile, FileDiff, Hunk, Modification};

/// The kind of FileDiff Header which can be used to print the FileDiff information which precedes
/// `Hunks`.
#[derive(Debug, Clone, PartialEq)]
pub enum FileHeader {
    Added {
        path: PathBuf,
        new: DiffFile,
    },
    Copied {
        old_path: PathBuf,
        new_path: PathBuf,
    },
    Deleted {
        path: PathBuf,
        old: DiffFile,
    },
    Modified {
        path: PathBuf,
        old: DiffFile,
        new: DiffFile,
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
            },
            FileDiff::Added(v) => FileHeader::Added {
                path: v.path.clone(),
                new: v.new.clone(),
            },
            FileDiff::Copied(_) => todo!(),
            FileDiff::Deleted(v) => FileHeader::Deleted {
                path: v.path.clone(),
                old: v.old.clone(),
            },
            FileDiff::Moved(v) => FileHeader::Moved {
                old_path: v.old_path.clone(),
                new_path: v.new_path.clone(),
            },
        }
    }
}

/// Meta data which precedes a `Hunk`s content.
///
/// For example:
/// @@ -24,8 +24,6 @@ use radicle_surf::diff::*;
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HunkHeader {
    /// Line the hunk started in the old file.
    pub old_line_no: usize,
    /// Number of removed and context lines.
    pub old_size: usize,
    /// Line the hunk started in the new file.
    pub new_line_no: usize,
    /// Number of added and context lines.
    pub new_size: usize,
    /// Trailing text for the Hunk Header.
    ///
    /// From Git's documentation "Hunk headers mention the name of the function to which the hunk
    /// applies. See "Defining a custom hunk-header" in gitattributes for details of how to tailor
    /// to this to specific languages.".  It is likely best to leave this empty when generating
    /// diffs.
    pub text: Vec<u8>,
}

/// A Trait for converting a value to its UnifiedDiff format.
pub trait UnifiedDiff: Sized {
    fn encode(&self, w: &mut Writer) -> io::Result<()>;

    fn to_unified_string(&self) -> String {
        let mut buf = Vec::new();

        {
            let mut w = Writer::new(&mut buf);
            w.encode(self).unwrap();
        }

        String::from_utf8(buf).unwrap()
    }
}

impl UnifiedDiff for Diff {
    fn encode(&self, w: &mut Writer) -> io::Result<()> {
        for fdiff in self.files() {
            fdiff.encode(w)?;
        }

        Ok(())
    }
}

impl UnifiedDiff for DiffContent {
    fn encode(&self, w: &mut Writer) -> io::Result<()> {
        match self {
            DiffContent::Plain { hunks, .. } => {
                for h in hunks.iter() {
                    h.encode(w)?;
                }
                Ok(())
            }
            DiffContent::Empty => Ok(()),
            DiffContent::Binary => unimplemented!(),
        }
    }
}

impl UnifiedDiff for FileDiff {
    fn encode(&self, w: &mut Writer) -> io::Result<()> {
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

impl UnifiedDiff for FileHeader {
    fn encode(&self, w: &mut Writer) -> io::Result<()> {
        match self {
            FileHeader::Modified { path, old, new } => {
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
            FileHeader::Added { path, new } => {
                w.meta(format!(
                    "diff --git a/{} b/{}",
                    path.display(),
                    path.display()
                ))?;

                w.meta(format!("new file mode {:o}", u32::from(new.mode.clone())))?;
                w.meta(format!(
                    "index {}..{}",
                    term::format::oid(Oid::zero()),
                    term::format::oid(new.oid),
                ))?;

                w.meta("--- /dev/null")?;
                w.meta(format!("+++ b/{}", path.display()))?;
            }
            FileHeader::Copied { .. } => todo!(),
            FileHeader::Deleted { path, old } => {
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
                    term::format::oid(Oid::zero())
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

impl UnifiedDiff for HunkHeader {
    fn encode(&self, w: &mut Writer) -> io::Result<()> {
        let old = if self.old_size == 1 {
            format!("{}", self.old_line_no)
        } else {
            format!("{},{}", self.old_line_no, self.old_size)
        };
        let new = if self.new_size == 1 {
            format!("{}", self.new_line_no)
        } else {
            format!("{},{}", self.new_line_no, self.new_size)
        };
        let text = if self.text.is_empty() {
            "".to_string()
        } else {
            format!(" {}", String::from_utf8_lossy(&self.text))
        };

        w.meta(format!("@@ -{old} +{new} @@{text}"))
    }
}

impl UnifiedDiff for Hunk<Modification> {
    fn encode(&self, w: &mut Writer) -> io::Result<()> {
        // TODO: Remove trailing newlines accurately.
        //   trim_end() will destroy diff information if the diff has a trailing whitespace on
        //   purpose.
        w.magenta(self.header.from_utf8_lossy().trim_end())?;
        for l in &self.lines {
            l.encode(w)?;
        }

        Ok(())
    }
}

impl UnifiedDiff for Modification {
    fn encode(&self, w: &mut Writer) -> io::Result<()> {
        match self {
            Modification::Deletion(radicle_surf::diff::Deletion { line, .. }) => {
                let s = format!("-{}", String::from_utf8_lossy(line.as_bytes()).trim_end());
                w.write(s, term::Style::new(term::Color::Red))
            }
            Modification::Addition(radicle_surf::diff::Addition { line, .. }) => {
                let s = format!("+{}", String::from_utf8_lossy(line.as_bytes()).trim_end());
                w.write(s, term::Style::new(term::Color::Green))
            }
            Modification::Context { line, .. } => {
                let s = format!(" {}", String::from_utf8_lossy(line.as_bytes()).trim_end());
                w.write(s, term::Style::default().dim())
            }
        }
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

    pub fn encode(&mut self, arg: &impl UnifiedDiff) -> io::Result<()> {
        arg.encode(self)
    }

    pub fn styled(mut self, value: bool) -> Self {
        self.styled = value;
        self
    }

    fn write(&mut self, s: impl fmt::Display, style: term::Style) -> io::Result<()> {
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
}
