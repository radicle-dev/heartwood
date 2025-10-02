use std::convert::TryFrom;

use super::{
    Diff, DiffContent, DiffFile, EofNewLine, FileMode, FileStats, Hunk, Hunks, Line, Modification,
    Stats,
};

pub mod error {
    use std::path::PathBuf;

    use thiserror::Error;

    #[derive(Debug, Error)]
    #[non_exhaustive]
    pub enum Addition {
        #[error(transparent)]
        Git(#[from] git2::Error),
        #[error("the new line number was missing for an added line")]
        MissingNewLineNo,
    }

    #[derive(Debug, Error)]
    #[non_exhaustive]
    pub enum Deletion {
        #[error(transparent)]
        Git(#[from] git2::Error),
        #[error("the new line number was missing for an deleted line")]
        MissingOldLineNo,
    }

    #[derive(Debug, Error)]
    #[non_exhaustive]
    pub enum FileMode {
        #[error("unknown file mode `{0:?}`")]
        Unknown(git2::FileMode),
    }

    #[derive(Debug, Error)]
    #[non_exhaustive]
    pub enum Modification {
        /// A Git `DiffLine` is invalid.
        #[error(
            "invalid `git2::DiffLine` which contains no line numbers for either side of the diff"
        )]
        Invalid,
    }

    #[derive(Debug, Error)]
    #[non_exhaustive]
    pub enum Hunk {
        #[error(transparent)]
        Git(#[from] git2::Error),
        #[error(transparent)]
        Line(#[from] Modification),
    }

    /// A Git diff error.
    #[derive(Debug, Error)]
    #[non_exhaustive]
    pub enum Diff {
        #[error(transparent)]
        Addition(#[from] Addition),
        #[error(transparent)]
        Deletion(#[from] Deletion),
        /// A Git delta type isn't currently handled.
        #[error("git delta type is not handled")]
        DeltaUnhandled(git2::Delta),
        #[error(transparent)]
        Git(#[from] git2::Error),
        #[error(transparent)]
        FileMode(#[from] FileMode),
        #[error(transparent)]
        Hunk(#[from] Hunk),
        #[error(transparent)]
        Line(#[from] Modification),
        /// A patch is unavailable.
        #[error("couldn't retrieve patch for {0}")]
        PatchUnavailable(PathBuf),
        /// A The path of a file isn't available.
        #[error("couldn't retrieve file path")]
        PathUnavailable,
    }
}

impl TryFrom<git2::DiffFile<'_>> for DiffFile {
    type Error = error::FileMode;

    fn try_from(value: git2::DiffFile) -> Result<Self, Self::Error> {
        Ok(Self {
            mode: value.mode().try_into()?,
            oid: value.id().into(),
        })
    }
}

impl TryFrom<git2::FileMode> for FileMode {
    type Error = error::FileMode;

    fn try_from(value: git2::FileMode) -> Result<Self, Self::Error> {
        match value {
            git2::FileMode::Blob => Ok(Self::Blob),
            git2::FileMode::BlobExecutable => Ok(Self::BlobExecutable),
            git2::FileMode::Commit => Ok(Self::Commit),
            git2::FileMode::Tree => Ok(Self::Tree),
            git2::FileMode::Link => Ok(Self::Link),
            _ => Err(error::FileMode::Unknown(value)),
        }
    }
}

impl From<FileMode> for git2::FileMode {
    fn from(m: FileMode) -> Self {
        match m {
            FileMode::Blob => git2::FileMode::Blob,
            FileMode::BlobExecutable => git2::FileMode::BlobExecutable,
            FileMode::Tree => git2::FileMode::Tree,
            FileMode::Link => git2::FileMode::Link,
            FileMode::Commit => git2::FileMode::Commit,
        }
    }
}

impl TryFrom<git2::Patch<'_>> for DiffContent {
    type Error = error::Hunk;

    fn try_from(patch: git2::Patch) -> Result<Self, Self::Error> {
        let mut hunks = Vec::new();
        let mut old_missing_eof = false;
        let mut new_missing_eof = false;
        let mut additions = 0;
        let mut deletions = 0;

        for h in 0..patch.num_hunks() {
            let (hunk, hunk_lines) = patch.hunk(h)?;
            let header = Line(hunk.header().to_owned());
            let mut lines: Vec<Modification> = Vec::new();

            for l in 0..hunk_lines {
                let line = patch.line_in_hunk(h, l)?;
                match line.origin_value() {
                    git2::DiffLineType::ContextEOFNL => {
                        new_missing_eof = true;
                        old_missing_eof = true;
                        continue;
                    }
                    git2::DiffLineType::Addition => {
                        additions += 1;
                    }
                    git2::DiffLineType::Deletion => {
                        deletions += 1;
                    }
                    git2::DiffLineType::AddEOFNL => {
                        additions += 1;
                        old_missing_eof = true;
                        continue;
                    }
                    git2::DiffLineType::DeleteEOFNL => {
                        deletions += 1;
                        new_missing_eof = true;
                        continue;
                    }
                    _ => {}
                }
                let line = Modification::try_from(line)?;
                lines.push(line);
            }
            hunks.push(Hunk {
                header,
                lines,
                old: hunk.old_start()..hunk.old_start() + hunk.old_lines(),
                new: hunk.new_start()..hunk.new_start() + hunk.new_lines(),
            });
        }
        let eof = match (old_missing_eof, new_missing_eof) {
            (true, true) => EofNewLine::BothMissing,
            (true, false) => EofNewLine::OldMissing,
            (false, true) => EofNewLine::NewMissing,
            (false, false) => EofNewLine::NoneMissing,
        };
        Ok(DiffContent::Plain {
            hunks: Hunks(hunks),
            stats: FileStats {
                additions,
                deletions,
            },
            eof,
        })
    }
}

impl TryFrom<git2::DiffLine<'_>> for Modification {
    type Error = error::Modification;

    fn try_from(line: git2::DiffLine) -> Result<Self, Self::Error> {
        match (line.old_lineno(), line.new_lineno()) {
            (None, Some(n)) => Ok(Self::addition(line.content().to_owned(), n)),
            (Some(n), None) => Ok(Self::deletion(line.content().to_owned(), n)),
            (Some(l), Some(r)) => Ok(Self::context(line.content().to_owned(), l, r)),
            (None, None) => Err(error::Modification::Invalid),
        }
    }
}

impl From<git2::DiffStats> for Stats {
    fn from(stats: git2::DiffStats) -> Self {
        Self {
            files_changed: stats.files_changed(),
            insertions: stats.insertions(),
            deletions: stats.deletions(),
        }
    }
}

impl TryFrom<git2::Diff<'_>> for Diff {
    type Error = error::Diff;

    fn try_from(git_diff: git2::Diff) -> Result<Diff, Self::Error> {
        use git2::Delta;

        let mut diff = Diff::new();

        // This allows libgit2 to run the binary detection.
        // Reference: <https://github.com/libgit2/libgit2/issues/6637>
        git_diff.foreach(&mut |_, _| true, None, None, None)?;

        for (idx, delta) in git_diff.deltas().enumerate() {
            match delta.status() {
                Delta::Added => created(&mut diff, &git_diff, idx, &delta)?,
                Delta::Deleted => deleted(&mut diff, &git_diff, idx, &delta)?,
                Delta::Modified => modified(&mut diff, &git_diff, idx, &delta)?,
                Delta::Renamed => renamed(&mut diff, &git_diff, idx, &delta)?,
                Delta::Copied => copied(&mut diff, &git_diff, idx, &delta)?,
                status => {
                    return Err(error::Diff::DeltaUnhandled(status));
                }
            }
        }

        Ok(diff)
    }
}

fn created(
    diff: &mut Diff,
    git_diff: &git2::Diff<'_>,
    idx: usize,
    delta: &git2::DiffDelta<'_>,
) -> Result<(), error::Diff> {
    let diff_file = delta.new_file();
    let is_binary = diff_file.is_binary();
    let path = diff_file
        .path()
        .ok_or(error::Diff::PathUnavailable)?
        .to_path_buf();
    let new = DiffFile::try_from(diff_file)?;

    let patch = git2::Patch::from_diff(git_diff, idx)?;
    if is_binary {
        diff.insert_added(path, DiffContent::Binary, new);
    } else if let Some(patch) = patch {
        diff.insert_added(path, DiffContent::try_from(patch)?, new);
    } else {
        return Err(error::Diff::PatchUnavailable(path));
    }
    Ok(())
}

fn deleted(
    diff: &mut Diff,
    git_diff: &git2::Diff<'_>,
    idx: usize,
    delta: &git2::DiffDelta<'_>,
) -> Result<(), error::Diff> {
    let diff_file = delta.old_file();
    let is_binary = diff_file.is_binary();
    let path = diff_file
        .path()
        .ok_or(error::Diff::PathUnavailable)?
        .to_path_buf();
    let patch = git2::Patch::from_diff(git_diff, idx)?;
    let old = DiffFile::try_from(diff_file)?;

    if is_binary {
        diff.insert_deleted(path, DiffContent::Binary, old);
    } else if let Some(patch) = patch {
        diff.insert_deleted(path, DiffContent::try_from(patch)?, old);
    } else {
        return Err(error::Diff::PatchUnavailable(path));
    }
    Ok(())
}

fn modified(
    diff: &mut Diff,
    git_diff: &git2::Diff<'_>,
    idx: usize,
    delta: &git2::DiffDelta<'_>,
) -> Result<(), error::Diff> {
    let diff_file = delta.new_file();
    let path = diff_file
        .path()
        .ok_or(error::Diff::PathUnavailable)?
        .to_path_buf();
    let patch = git2::Patch::from_diff(git_diff, idx)?;
    let old = DiffFile::try_from(delta.old_file())?;
    let new = DiffFile::try_from(delta.new_file())?;

    if diff_file.is_binary() {
        diff.insert_modified(path, DiffContent::Binary, old, new);
        Ok(())
    } else if let Some(patch) = patch {
        diff.insert_modified(path, DiffContent::try_from(patch)?, old, new);
        Ok(())
    } else {
        Err(error::Diff::PatchUnavailable(path))
    }
}

fn renamed(
    diff: &mut Diff,
    git_diff: &git2::Diff<'_>,
    idx: usize,
    delta: &git2::DiffDelta<'_>,
) -> Result<(), error::Diff> {
    let old_path = delta
        .old_file()
        .path()
        .ok_or(error::Diff::PathUnavailable)?
        .to_path_buf();
    let new_path = delta
        .new_file()
        .path()
        .ok_or(error::Diff::PathUnavailable)?
        .to_path_buf();
    let patch = git2::Patch::from_diff(git_diff, idx)?;
    let old = DiffFile::try_from(delta.old_file())?;
    let new = DiffFile::try_from(delta.new_file())?;

    if delta.new_file().is_binary() {
        diff.insert_moved(old_path, new_path, old, new, DiffContent::Binary);
    } else if let Some(patch) = patch {
        diff.insert_moved(old_path, new_path, old, new, DiffContent::try_from(patch)?);
    } else {
        diff.insert_moved(old_path, new_path, old, new, DiffContent::Empty);
    }
    Ok(())
}

fn copied(
    diff: &mut Diff,
    git_diff: &git2::Diff<'_>,
    idx: usize,
    delta: &git2::DiffDelta<'_>,
) -> Result<(), error::Diff> {
    let old_path = delta
        .old_file()
        .path()
        .ok_or(error::Diff::PathUnavailable)?
        .to_path_buf();
    let new_path = delta
        .new_file()
        .path()
        .ok_or(error::Diff::PathUnavailable)?
        .to_path_buf();
    let patch = git2::Patch::from_diff(git_diff, idx)?;
    let old = DiffFile::try_from(delta.old_file())?;
    let new = DiffFile::try_from(delta.new_file())?;

    if delta.new_file().is_binary() {
        diff.insert_copied(old_path, new_path, old, new, DiffContent::Binary);
    } else if let Some(patch) = patch {
        diff.insert_copied(old_path, new_path, old, new, DiffContent::try_from(patch)?);
    } else {
        diff.insert_copied(old_path, new_path, old, new, DiffContent::Empty);
    }
    Ok(())
}
