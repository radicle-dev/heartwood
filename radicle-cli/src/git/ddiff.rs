//! `DDiff` is a diff between diffs.  The type aids in the review of a `Patch` to a project by
//! providing useful context between `Patch` updates a regular `Diff` will miss.
//!
//! For example, lets start with a file containing a list of words.
//!
//! ```text
//! componentwise
//! reusing
//! simplest
//! crag
//! offended
//! omitting
//! ```
//! Where a change is proposed to the file replacing a set of lines.  The example includes the
//! `HunkHeader` "@ .. @" for completeness, but it can be mostly ignored.
//!
//! ```text
//! @@ -0,6 +0,6 @@
//! componentwise
//! reusing
//! -simplest
//! -crag
//! -offended
//! +interpreters
//! +soiled
//! +snuffing
//! omitting
//! ```
//!
//! The author updates the `Patch` to keep 'offended' and remove 'interpreters'.
//!
//! ```text
//! @@ -0,6 +0,6 @@
//! componentwise
//! reusing
//! -simplest
//! -crag
//!  offended
//! -interpreters
//! +soiled
//! +snuffing
//! omitting
//! ```
//! The `DDiff` will show the what changes are being made, overlayed on to the original diff and
//! the diff's original file as context.
//!
//! ```text
//! @@ -0,9 +0,8 @@
//!   componentwise
//!   reusing
//!  -simplest
//!  -crag
//! --offended
//! + offended
//! -+interpreters
//!  +soiled
//!  +snuffing
//!   omitting
//! ```
//!
//! An alternative is to review a `Diff` between the resulting files after the first and second
//! Patch versions were applied.  The first `Patch` changes and original file contents are one
//! making it unclear what are changes to the `Patch` or changes to the original file.
//!
//! ```text
//! @@ -0,9 +0,8 @@
//!  componentwise
//!  reusing
//! +offended
//! -interpreters
//!  soiled
//!  snuffing
//!  omitting
//! ```
use radicle_surf::diff::*;

use std::io;

use crate::git::unified_diff;
use crate::git::unified_diff::{Encode, Writer};
use crate::terminal as term;

/// Either the modification of a single diff [`Line`], or just contextual
/// information.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffModification {
    /// An addition line is to be added.
    AdditionAddition { line: Line, line_no: u32 },
    AdditionContext {
        line: Line,
        line_no_old: u32,
        line_no_new: u32,
    },
    /// An addition line is to be removed.
    AdditionDeletion { line: Line, line_no: u32 },
    /// A context line is to be added.
    ContextAddition { line: Line, line_no: u32 },
    /// A contextual line in a file, i.e. there were no changes to the line.
    ContextContext {
        line: Line,
        line_no_old: u32,
        line_no_new: u32,
    },
    /// A context line is to be removed.
    ContextDeletion { line: Line, line_no: u32 },
    /// A deletion line is to be added.
    DeletionAddition { line: Line, line_no: u32 },
    /// A deletion line in a diff, i.e. there were no changes to the line.
    DeletionContext {
        line: Line,
        line_no_old: u32,
        line_no_new: u32,
    },
    /// A deletion line is to be removed.
    DeletionDeletion { line: Line, line_no: u32 },
}

impl unified_diff::Decode for Hunk<DiffModification> {
    fn decode(r: &mut impl io::BufRead) -> Result<Self, unified_diff::Error> {
        let header = unified_diff::HunkHeader::decode(r)?;

        let mut lines = Vec::new();
        let mut new_line: u32 = 0;
        let mut old_line: u32 = 0;

        while old_line < header.old_size || new_line < header.new_size {
            if old_line > header.old_size {
                return Err(unified_diff::Error::syntax(format!(
                    "expected '{0}' old lines",
                    header.old_size,
                )));
            } else if new_line > header.new_size {
                return Err(unified_diff::Error::syntax(format!(
                    "expected '{0}' new lines",
                    header.new_size,
                )));
            }

            let mut line = DiffModification::decode(r).map_err(|e| {
                if e.is_eof() {
                    unified_diff::Error::syntax(format!(
                        "expected '{}' old lines and '{}' new lines, but found '{}' and '{}'",
                        header.old_size, header.new_size, old_line, new_line,
                    ))
                } else {
                    e
                }
            })?;

            match &mut line {
                DiffModification::AdditionAddition { line_no, .. } => {
                    *line_no = new_line;
                    new_line += 1;
                }
                DiffModification::AdditionContext {
                    line_no_old,
                    line_no_new,
                    ..
                } => {
                    *line_no_old = old_line;
                    *line_no_new = new_line;
                    old_line += 1;
                    new_line += 1;
                }
                DiffModification::AdditionDeletion { line_no, .. } => {
                    *line_no = old_line;
                    old_line += 1;
                }
                DiffModification::ContextAddition { line_no, .. } => {
                    *line_no = new_line;
                    new_line += 1;
                }
                DiffModification::ContextContext {
                    line_no_old,
                    line_no_new,
                    ..
                } => {
                    *line_no_old = old_line;
                    *line_no_new = new_line;
                    old_line += 1;
                    new_line += 1;
                }
                DiffModification::ContextDeletion { line_no, .. } => {
                    *line_no = old_line;
                    old_line += 1;
                }
                DiffModification::DeletionAddition { line_no, .. } => {
                    *line_no = new_line;
                    new_line += 1;
                }
                DiffModification::DeletionContext {
                    line_no_old,
                    line_no_new,
                    ..
                } => {
                    *line_no_old = old_line;
                    *line_no_new = new_line;
                    old_line += 1;
                    new_line += 1;
                }
                DiffModification::DeletionDeletion { line_no, .. } => {
                    *line_no = old_line;
                    old_line += 1;
                }
            };

            lines.push(line);
        }

        Ok(Hunk {
            header: Line::from(header.to_unified_string()?),
            lines,
            old: header.old_line_range(),
            new: header.new_line_range(),
        })
    }
}

impl unified_diff::Encode for Hunk<DiffModification> {
    fn encode(&self, w: &mut Writer) -> Result<(), unified_diff::Error> {
        // TODO: Remove trailing newlines accurately.
        // trim_end() will destroy diff information if the diff has a trailing whitespace on
        // purpose.
        w.magenta(self.header.from_utf8_lossy().trim_end())?;
        for l in &self.lines {
            l.encode(w)?;
        }
        Ok(())
    }
}

/// The DDiff version of `FileDiff`.
#[derive(Clone, Debug, PartialEq)]
pub struct FileDDiff {
    pub path: std::path::PathBuf,
    pub old: DiffFile,
    pub new: DiffFile,
    pub hunks: Hunks<DiffModification>,
    pub eof: EofNewLine,
}

impl From<&FileDDiff> for unified_diff::FileHeader {
    fn from(value: &FileDDiff) -> Self {
        unified_diff::FileHeader::Modified {
            path: value.path.clone(),
            old: value.old.clone(),
            new: value.new.clone(),
        }
    }
}

impl unified_diff::Decode for DiffModification {
    fn decode(r: &mut impl std::io::BufRead) -> Result<Self, unified_diff::Error> {
        let mut line = String::new();
        if r.read_line(&mut line)? == 0 {
            return Err(unified_diff::Error::UnexpectedEof);
        }

        let mut chars = line.chars();

        let first = chars.next().ok_or(unified_diff::Error::UnexpectedEof)?;
        let second = chars.next().ok_or(unified_diff::Error::UnexpectedEof)?;

        let line = match (first, second) {
            ('+', '+') => DiffModification::AdditionAddition {
                line: chars.as_str().to_string().into(),
                line_no: 0,
            },
            ('+', '-') => DiffModification::DeletionDeletion {
                line: chars.as_str().to_string().into(),
                line_no: 0,
            },
            ('+', ' ') => DiffModification::ContextAddition {
                line: chars.as_str().to_string().into(),
                line_no: 0,
            },
            ('-', '+') => DiffModification::AdditionDeletion {
                line: chars.as_str().to_string().into(),
                line_no: 0,
            },
            ('-', '-') => DiffModification::DeletionDeletion {
                line: chars.as_str().to_string().into(),
                line_no: 0,
            },
            ('-', ' ') => DiffModification::ContextDeletion {
                line: chars.as_str().to_string().into(),
                line_no: 0,
            },
            (' ', '+') => DiffModification::AdditionContext {
                line: chars.as_str().to_string().into(),
                line_no_old: 0,
                line_no_new: 0,
            },
            (' ', '-') => DiffModification::DeletionContext {
                line: chars.as_str().to_string().into(),
                line_no_old: 0,
                line_no_new: 0,
            },
            (' ', ' ') => DiffModification::ContextContext {
                line: chars.as_str().to_string().into(),
                line_no_old: 0,
                line_no_new: 0,
            },
            (v1, v2) => {
                return Err(unified_diff::Error::syntax(format!(
                    "indicator character expected, but got '{0}{1}'",
                    v1, v2
                )))
            }
        };

        Ok(line)
    }
}

impl unified_diff::Encode for DiffModification {
    fn encode(&self, w: &mut unified_diff::Writer) -> Result<(), unified_diff::Error> {
        match self {
            DiffModification::AdditionAddition { line, .. } => {
                let s = format!("++{}", String::from_utf8_lossy(line.as_bytes()).trim_end());
                w.write(s, term::Style::new(term::Color::Green))?;
            }
            DiffModification::AdditionDeletion { line, .. } => {
                let s = format!("-+{}", String::from_utf8_lossy(line.as_bytes()).trim_end());
                w.write(s, term::Style::new(term::Color::Red))?;
            }
            DiffModification::ContextAddition { line, .. } => {
                let s = format!("+ {}", String::from_utf8_lossy(line.as_bytes()).trim_end());
                w.write(s, term::Style::new(term::Color::Green))?;
            }
            DiffModification::DeletionAddition { line, .. } => {
                let s = format!("+-{}", String::from_utf8_lossy(line.as_bytes()).trim_end());
                w.write(s, term::Style::new(term::Color::Green))?;
            }
            DiffModification::DeletionDeletion { line, .. } => {
                let s = format!("--{}", String::from_utf8_lossy(line.as_bytes()).trim_end());
                w.write(s, term::Style::new(term::Color::Red))?;
            }
            DiffModification::ContextDeletion { line, .. } => {
                let s = format!("- {}", String::from_utf8_lossy(line.as_bytes()).trim_end());
                w.write(s, term::Style::new(term::Color::Red))?;
            }
            DiffModification::AdditionContext { line, .. } => {
                let s = format!(" +{}", String::from_utf8_lossy(line.as_bytes()).trim_end());
                w.write(s, term::Style::new(term::Color::Green).dim())?
            }
            DiffModification::DeletionContext { line, .. } => {
                let s = format!(" -{}", String::from_utf8_lossy(line.as_bytes()).trim_end());
                w.write(s, term::Style::new(term::Color::Red).dim())?;
            }
            DiffModification::ContextContext { line, .. } => {
                let s = format!("  {}", String::from_utf8_lossy(line.as_bytes()).trim_end());
                w.write(s, term::Style::default().dim())?;
            }
        }

        Ok(())
    }
}

impl unified_diff::Encode for FileDDiff {
    fn encode(&self, w: &mut unified_diff::Writer) -> Result<(), unified_diff::Error> {
        w.encode(&unified_diff::FileHeader::from(self))?;
        for h in self.hunks.iter() {
            h.encode(w)?;
        }

        Ok(())
    }
}

/// A diff of a diff.
#[derive(Clone, Debug, PartialEq)]
pub struct DDiff {
    files: Vec<FileDDiff>,
}

impl DDiff {
    /// Returns an iterator of the file in the diff.
    pub fn files(&self) -> impl Iterator<Item = &FileDDiff> {
        self.files.iter()
    }

    /// Returns owned files in the diff.
    pub fn into_files(self) -> Vec<FileDDiff> {
        self.files
    }
}

impl unified_diff::Encode for DDiff {
    fn encode(&self, w: &mut unified_diff::Writer) -> Result<(), unified_diff::Error> {
        for v in self.files() {
            v.encode(w)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::git::unified_diff::{Decode, Encode};

    #[test]
    fn diff_encode_decode_ddiff_hunk() {
        let ddiff = Hunk::<DiffModification>::parse(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/data/ddiff_hunk.diff"
        )))
        .unwrap();
        assert_eq!(
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/data/ddiff_hunk.diff"
            )),
            ddiff.to_unified_string().unwrap()
        );
    }
}
