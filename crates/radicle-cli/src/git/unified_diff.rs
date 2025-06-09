//! Formatting support for Git's [diff format](https://git-scm.com/docs/diff-format).
use std::fmt;
use std::io;
use std::path::PathBuf;

use radicle_surf::diff::FileStats;
use thiserror::Error;

use radicle::git;
use radicle::git::raw::Oid;
use radicle_surf::diff;
use radicle_surf::diff::{Diff, DiffContent, DiffFile, FileDiff, Hunk, Hunks, Line, Modification};

use crate::terminal as term;

#[derive(Debug, Error)]
pub enum Error {
    /// Attempt to decode from a source with no data left.
    #[error("unexpected end of file")]
    UnexpectedEof,
    #[error(transparent)]
    Io(#[from] io::Error),
    /// Catchall for syntax error messages.
    #[error("{0}")]
    Syntax(String),
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
}

impl Error {
    pub fn syntax(msg: impl ToString) -> Self {
        Self::Syntax(msg.to_string())
    }

    pub fn is_eof(&self) -> bool {
        match self {
            Self::UnexpectedEof => true,
            Self::Io(e) => e.kind() == io::ErrorKind::UnexpectedEof,
            _ => false,
        }
    }
}

/// The kind of FileDiff Header which can be used to print the FileDiff information which precedes
/// `Hunks`.
#[derive(Debug, Clone, PartialEq)]
pub enum FileHeader {
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

/// Meta data which precedes a `Hunk`s content.
///
/// For example:
/// @@ -24,8 +24,6 @@ use radicle_surf::diff::*;
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HunkHeader {
    /// Line the hunk started in the old file.
    pub old_line_no: u32,
    /// Number of removed and context lines.
    pub old_size: u32,
    /// Line the hunk started in the new file.
    pub new_line_no: u32,
    /// Number of added and context lines.
    pub new_size: u32,
    /// Trailing text for the Hunk Header.
    ///
    /// From Git's documentation "Hunk headers mention the name of the function to which the hunk
    /// applies. See "Defining a custom hunk-header" in gitattributes for details of how to tailor
    /// to this to specific languages.".  It is likely best to leave this empty when generating
    /// diffs.
    pub text: Vec<u8>,
}

impl TryFrom<&Hunk<Modification>> for HunkHeader {
    type Error = Error;

    fn try_from(hunk: &Hunk<Modification>) -> Result<Self, Self::Error> {
        let mut r = io::BufReader::new(hunk.header.as_bytes());
        Self::decode(&mut r)
    }
}

impl HunkHeader {
    pub fn old_line_range(&self) -> std::ops::Range<u32> {
        let start: u32 = self.old_line_no;
        let end: u32 = self.old_line_no + self.old_size;
        start..end + 1
    }

    pub fn new_line_range(&self) -> std::ops::Range<u32> {
        let start: u32 = self.new_line_no;
        let end: u32 = self.new_line_no + self.new_size;
        start..end + 1
    }
}

/// Diff-related types that can be decoded from the unified diff format.
pub trait Decode: Sized {
    /// Decode, and fail if we reach the end of the stream.
    fn decode(r: &mut impl io::BufRead) -> Result<Self, Error>;

    /// Decode, and return a `None` if we reached the end of the stream.
    fn try_decode(r: &mut impl io::BufRead) -> Result<Option<Self>, Error> {
        match Self::decode(r) {
            Ok(v) => Ok(Some(v)),
            Err(Error::UnexpectedEof) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Decode from a string input.
    fn parse(s: &str) -> Result<Self, Error> {
        Self::from_bytes(s.as_bytes())
    }

    /// Decode from a string input.
    fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let mut r = io::BufReader::new(bytes);
        Self::decode(&mut r)
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

impl Decode for Diff {
    /// Decode from git's unified diff format, consuming the entire input.
    fn decode(r: &mut impl io::BufRead) -> Result<Self, Error> {
        let mut s = String::new();

        r.read_to_string(&mut s)?;

        let d = git::raw::Diff::from_buffer(s.as_ref())
            .map_err(|e| Error::syntax(format!("decoding unified diff: {}", e)))?;
        let d = Diff::try_from(d)
            .map_err(|e| Error::syntax(format!("decoding unified diff: {}", e)))?;

        Ok(d)
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

impl Decode for DiffContent {
    fn decode(r: &mut impl io::BufRead) -> Result<Self, Error> {
        let mut hunks = Vec::default();
        let mut additions = 0;
        let mut deletions = 0;

        while let Some(h) = Hunk::try_decode(r)? {
            for l in &h.lines {
                match l {
                    Modification::Addition(_) => additions += 1,
                    Modification::Deletion(_) => deletions += 1,
                    _ => {}
                }
            }
            hunks.push(h);
        }

        if hunks.is_empty() {
            Ok(DiffContent::Empty)
        } else {
            // TODO: Handle case for binary.
            Ok(DiffContent::Plain {
                hunks: Hunks::from(hunks),
                stats: FileStats {
                    additions,
                    deletions,
                },
                // TODO: Properly handle EndOfLine field
                eof: diff::EofNewLine::NoneMissing,
            })
        }
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
                    term::format::oid(Oid::zero()),
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

impl Decode for HunkHeader {
    fn decode(r: &mut impl io::BufRead) -> Result<Self, Error> {
        let mut line = String::default();
        if r.read_line(&mut line)? == 0 {
            return Err(Error::UnexpectedEof);
        };

        let mut header = HunkHeader::default();
        let s = line
            .strip_prefix("@@ -")
            .ok_or(Error::syntax("missing '@@ -'"))?;

        let (old, s) = s
            .split_once(" +")
            .ok_or(Error::syntax("missing new line information"))?;
        let (line_no, size) = old.split_once(',').unwrap_or((old, "1"));

        header.old_line_no = line_no.parse()?;
        header.old_size = size.parse()?;

        let (new, s) = s
            .split_once(" @@")
            .ok_or(Error::syntax("closing '@@' is missing"))?;
        let (line_no, size) = new.split_once(',').unwrap_or((new, "1"));

        header.new_line_no = line_no.parse()?;
        header.new_size = size.parse()?;

        let s = s.strip_prefix(' ').unwrap_or(s);
        header.text = s.as_bytes().to_vec();

        Ok(header)
    }
}

impl Encode for HunkHeader {
    fn encode(&self, w: &mut Writer) -> Result<(), Error> {
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
        w.meta(format!("@@ -{old} +{new} @@{text}"))?;

        Ok(())
    }
}

impl Decode for Hunk<Modification> {
    fn decode(r: &mut impl io::BufRead) -> Result<Self, Error> {
        let header = HunkHeader::decode(r)?;

        let mut lines = Vec::new();
        let mut new_line: u32 = 0;
        let mut old_line: u32 = 0;

        while old_line < header.old_size || new_line < header.new_size {
            if old_line > header.old_size {
                return Err(Error::syntax(format!(
                    "expected '{}' old lines",
                    header.old_size
                )));
            } else if new_line > header.new_size {
                return Err(Error::syntax(format!(
                    "expected '{0}' new lines",
                    header.new_size
                )));
            }

            let Some(line) = Modification::try_decode(r)? else {
                return Err(Error::syntax(format!(
                    "expected '{}' old lines and '{}' new lines, but found '{}' and '{}'",
                    header.old_size, header.new_size, old_line, new_line,
                )));
            };

            let line = match line {
                Modification::Addition(v) => {
                    let l = Modification::addition(v.line, header.new_line_no + new_line);
                    new_line += 1;
                    l
                }
                Modification::Deletion(v) => {
                    let l = Modification::deletion(v.line, header.old_line_no + old_line);
                    old_line += 1;
                    l
                }
                Modification::Context { line, .. } => {
                    let l = Modification::Context {
                        line,
                        line_no_old: header.old_line_no + old_line,
                        line_no_new: header.new_line_no + new_line,
                    };
                    new_line += 1;
                    old_line += 1;
                    l
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

impl Decode for Modification {
    fn decode(r: &mut impl io::BufRead) -> Result<Self, Error> {
        let mut line = String::new();
        if r.read_line(&mut line)? == 0 {
            return Err(Error::UnexpectedEof);
        };

        let mut chars = line.chars();
        let l = match chars.next() {
            Some('+') => Modification::addition(chars.as_str().to_string(), 0),
            Some('-') => Modification::deletion(chars.as_str().to_string(), 0),
            Some(' ') => Modification::Context {
                line: chars.as_str().to_string().into(),
                line_no_old: 0,
                line_no_new: 0,
            },
            Some(c) => {
                return Err(Error::syntax(format!(
                    "indicator character expected, but got '{c}'",
                )))
            }
            None => return Err(Error::UnexpectedEof),
        };

        Ok(l)
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

    pub fn styled(mut self, value: bool) -> Self {
        self.styled = value;
        self
    }

    pub fn write(&mut self, s: impl fmt::Display, style: term::Style) -> io::Result<()> {
        if self.styled {
            writeln!(self.stream, "{}", term::Paint::new(s).with_style(style))
        } else {
            writeln!(self.stream, "{s}")
        }
    }

    pub fn meta(&mut self, s: impl fmt::Display) -> io::Result<()> {
        self.write(s, term::Style::new(term::Color::Yellow))
    }

    pub fn magenta(&mut self, s: impl fmt::Display) -> io::Result<()> {
        self.write(s, term::Style::new(term::Color::Magenta))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_diff_encode_decode_diff() {
        let diff_a = diff::Diff::parse(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/data/diff.diff"
        )))
        .unwrap();
        assert_eq!(
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/diff.diff")),
            diff_a.to_unified_string().unwrap()
        );
    }

    #[test]
    fn test_diff_content_encode_decode_content() {
        let diff_content = diff::DiffContent::parse(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/data/diff_body.diff"
        )))
        .unwrap();
        assert_eq!(
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/data/diff_body.diff"
            )),
            diff_content.to_unified_string().unwrap()
        );
    }

    // TODO: Test parsing a real diff from this repository.
}
