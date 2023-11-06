use std::fs;
use std::path::Path;

use radicle::git;
use radicle_surf::diff;
use radicle_surf::diff::{Diff, DiffContent, FileDiff, Hunk, Modification};
use radicle_term as term;
use term::cell::Cell;

use crate::git::unified_diff::FileHeader;
use crate::terminal::highlight::{Highlighter, Theme};

use super::unified_diff::{Decode, HunkHeader};

/// Blob returned by the [`Repo`] trait.
pub enum Blob {
    Binary,
    Plain(Vec<u8>),
}

/// A repository of Git blobs.
pub trait Repo {
    /// Lookup a blob from the repo.
    fn blob(&self, oid: git::Oid) -> Result<Blob, git::raw::Error>;
    /// Lookup a file in the workdir.
    fn file(&self, path: &Path) -> Option<Blob>;
}

impl Repo for git::raw::Repository {
    fn blob(&self, oid: git::Oid) -> Result<Blob, git::raw::Error> {
        let blob = self.find_blob(*oid)?;

        if blob.is_binary() {
            Ok(Blob::Binary)
        } else {
            Ok(Blob::Plain(blob.content().to_vec()))
        }
    }

    fn file(&self, path: &Path) -> Option<Blob> {
        self.workdir()
            .and_then(|dir| fs::read(dir.join(path)).ok())
            .map(|content| {
                // A file is considered binary if there is a zero byte in the first 8 kilobytes
                // of the file. This is the same heuristic Git uses.
                let binary = content.iter().take(8192).any(|b| *b == 0);
                if binary {
                    Blob::Binary
                } else {
                    Blob::Plain(content)
                }
            })
    }
}

/// Blobs passed down to the hunk renderer.
#[derive(Debug, Default)]
pub struct Blobs {
    old: Option<Vec<term::Line>>,
    new: Option<Vec<term::Line>>,
}

/// Types that can be rendered as pretty diffs.
pub trait ToPretty {
    /// The output of the render process.
    type Output: term::Element;
    /// Context that can be passed down from parent objects during rendering.
    type Context;

    /// Render to pretty diff output.
    fn pretty<R: Repo>(
        &self,
        hi: &mut Highlighter,
        context: &Self::Context,
        repo: &R,
    ) -> Self::Output;
}

impl ToPretty for Diff {
    type Output = term::VStack<'static>;
    type Context = ();

    fn pretty<R: Repo>(
        &self,
        hi: &mut Highlighter,
        context: &Self::Context,
        repo: &R,
    ) -> Self::Output {
        term::VStack::default()
            .padding(0)
            .children(self.files().map(|f| f.pretty(hi, context, repo).boxed()))
    }
}

impl ToPretty for FileHeader {
    type Output = term::Line;
    type Context = ();

    fn pretty<R: Repo>(
        &self,
        _hi: &mut Highlighter,
        _context: &Self::Context,
        _repo: &R,
    ) -> Self::Output {
        match self {
            FileHeader::Added { path, .. } => term::Line::new(path.display().to_string()),
            FileHeader::Moved {
                old_path, new_path, ..
            } => term::Line::spaced([
                term::label(old_path.display().to_string()),
                term::label("->".to_string()),
                term::label(new_path.display().to_string()),
            ]),
            FileHeader::Deleted { path, .. } => term::Line::new(path.display().to_string()),
            FileHeader::Modified { path, .. } => term::Line::new(path.display().to_string()),
            FileHeader::Copied {
                old_path, new_path, ..
            } => term::Line::spaced([
                term::label(old_path.display().to_string()),
                term::label("->".to_string()),
                term::label(new_path.display().to_string()),
            ]),
        }
    }
}

impl ToPretty for FileDiff {
    type Output = term::VStack<'static>;
    type Context = ();

    fn pretty<R: Repo>(
        &self,
        hi: &mut Highlighter,
        _context: &Self::Context,
        repo: &R,
    ) -> Self::Output {
        let content = match self {
            FileDiff::Added(f) => f.diff.pretty(hi, self, repo),
            FileDiff::Moved(f) => f.diff.pretty(hi, self, repo),
            FileDiff::Deleted(f) => f.diff.pretty(hi, self, repo),
            FileDiff::Modified(f) => f.diff.pretty(hi, self, repo),
            FileDiff::Copied(f) => f.diff.pretty(hi, self, repo),
        };
        term::VStack::default()
            .padding(0)
            .child(content)
            .child(term::Line::blank())
    }
}

impl ToPretty for DiffContent {
    type Output = term::VStack<'static>;
    type Context = FileDiff;

    fn pretty<R: Repo>(
        &self,
        hi: &mut Highlighter,
        context: &Self::Context,
        repo: &R,
    ) -> Self::Output {
        let header = FileHeader::from(context);
        let theme = Theme::default();

        let (old, new, badge) = match context {
            FileDiff::Added(f) => (
                None,
                Some((f.new.oid, f.path.clone())),
                Some(term::format::badge_positive("created")),
            ),
            FileDiff::Moved(f) => (
                Some((f.old.oid, f.old_path.clone())),
                Some((f.new.oid, f.new_path.clone())),
                Some(term::format::badge_secondary("moved")),
            ),
            FileDiff::Deleted(f) => (
                Some((f.old.oid, f.path.clone())),
                None,
                Some(term::format::badge_negative("deleted")),
            ),
            FileDiff::Modified(f) => (
                Some((f.old.oid, f.path.clone())),
                Some((f.new.oid, f.path.clone())),
                None,
            ),
            FileDiff::Copied(f) => (
                Some((f.old.oid, f.old_path.clone())),
                Some((f.old.oid, f.new_path.clone())),
                Some(term::format::badge_secondary("copied")),
            ),
        };
        let mut header = header.pretty(hi, &(), repo);

        let (additions, deletions) = if let Some(stats) = self.stats() {
            (stats.additions, stats.deletions)
        } else {
            (0, 0)
        };

        if deletions > 0 {
            header.push(term::Label::space());
            header.push(term::label(format!("-{deletions}")).fg(theme.color("negative.light")));
        }
        if additions > 0 {
            header.push(term::Label::space());
            header.push(term::label(format!("+{additions}")).fg(theme.color("positive.light")));
        }
        if let Some(badge) = badge {
            header.push(term::Label::space());
            header.push(badge);
        }

        let old = old.and_then(|(oid, path)| repo.blob(oid).ok().or_else(|| repo.file(&path)));
        let new = new.and_then(|(oid, path)| repo.blob(oid).ok().or_else(|| repo.file(&path)));
        let mut blobs = Blobs::default();

        if let Some(Blob::Plain(content)) = old {
            blobs.old = hi.highlight(context.path(), &content).ok().flatten();
        }
        if let Some(Blob::Plain(content)) = new {
            blobs.new = hi.highlight(context.path(), &content).ok().flatten();
        }
        let mut vstack = term::VStack::default()
            .border(Some(term::colors::FAINT))
            .padding(1)
            .child(term::Line::default().extend(header));

        match context {
            FileDiff::Moved(_) | FileDiff::Copied(_) => {}
            FileDiff::Added(_) | FileDiff::Deleted(_) | FileDiff::Modified(_) => {
                vstack = vstack.divider();

                match self {
                    DiffContent::Plain { hunks, .. } => {
                        for (i, h) in hunks.iter().enumerate() {
                            vstack.push(h.pretty(hi, &blobs, repo));
                            if i != hunks.0.len() - 1 {
                                vstack = vstack.divider();
                            }
                        }
                    }
                    DiffContent::Empty => {
                        vstack.push(term::Line::new(term::format::italic("Empty file")));
                    }
                    DiffContent::Binary => {
                        vstack.push(term::Line::new(term::format::italic("Binary file")));
                    }
                }
            }
        }
        vstack
    }
}

impl ToPretty for HunkHeader {
    type Output = term::Line;
    type Context = ();

    fn pretty<R: Repo>(
        &self,
        _hi: &mut Highlighter,
        _context: &Self::Context,
        _repo: &R,
    ) -> Self::Output {
        term::Line::spaced([
            term::label(format!(
                "@@ -{},{} +{},{} @@",
                self.old_line_no, self.old_size, self.new_line_no, self.new_size,
            ))
            .fg(term::colors::fixed::FAINT),
            term::label(String::from_utf8_lossy(&self.text).to_string())
                .fg(term::colors::fixed::DIM),
        ])
    }
}

impl ToPretty for Hunk<Modification> {
    type Output = term::VStack<'static>;
    type Context = Blobs;

    fn pretty<R: Repo>(&self, hi: &mut Highlighter, blobs: &Blobs, repo: &R) -> Self::Output {
        let mut vstack = term::VStack::default().padding(0);
        let mut table = term::Table::<5, term::Filled<term::Line>>::new(term::TableOptions {
            overflow: false,
            spacing: 0,
            border: None,
        });
        let theme = Theme::default();

        if let Ok(header) = HunkHeader::from_bytes(self.header.as_bytes()) {
            vstack.push(header.pretty(hi, &(), repo));
        }
        for line in &self.lines {
            match line {
                Modification::Addition(a) => {
                    table.push([
                        term::Label::space()
                            .pad(5)
                            .bg(theme.color("positive"))
                            .to_line()
                            .filled(theme.color("positive")),
                        term::label(a.line_no.to_string())
                            .pad(5)
                            .fg(theme.color("positive.light"))
                            .to_line()
                            .filled(theme.color("positive")),
                        term::label(" + ")
                            .fg(theme.color("positive.light"))
                            .to_line()
                            .filled(theme.color("positive.dark")),
                        line.pretty(hi, blobs, repo)
                            .filled(theme.color("positive.dark")),
                        term::Line::blank().filled(term::Color::default()),
                    ]);
                }
                Modification::Deletion(a) => {
                    table.push([
                        term::label(a.line_no.to_string())
                            .pad(5)
                            .fg(theme.color("negative.light"))
                            .to_line()
                            .filled(theme.color("negative")),
                        term::Label::space()
                            .pad(5)
                            .fg(theme.color("dim"))
                            .to_line()
                            .filled(theme.color("negative")),
                        term::label(" - ")
                            .fg(theme.color("negative.light"))
                            .to_line()
                            .filled(theme.color("negative.dark")),
                        line.pretty(hi, blobs, repo)
                            .filled(theme.color("negative.dark")),
                        term::Line::blank().filled(term::Color::default()),
                    ]);
                }
                Modification::Context {
                    line_no_old,
                    line_no_new,
                    ..
                } => {
                    table.push([
                        term::label(line_no_old.to_string())
                            .pad(5)
                            .fg(theme.color("dim"))
                            .to_line()
                            .filled(theme.color("faint")),
                        term::label(line_no_new.to_string())
                            .pad(5)
                            .fg(theme.color("dim"))
                            .to_line()
                            .filled(theme.color("faint")),
                        term::label("   ").to_line().filled(term::Color::default()),
                        line.pretty(hi, blobs, repo).filled(term::Color::default()),
                        term::Line::blank().filled(term::Color::default()),
                    ]);
                }
            }
        }
        vstack.push(table);
        vstack
    }
}

impl ToPretty for Modification {
    type Output = term::Line;
    type Context = Blobs;

    fn pretty<R: Repo>(&self, _hi: &mut Highlighter, blobs: &Blobs, _repo: &R) -> Self::Output {
        match self {
            Modification::Deletion(diff::Deletion { line, line_no }) => {
                if let Some(lines) = &blobs.old.as_ref() {
                    lines[*line_no as usize - 1].clone()
                } else {
                    term::Line::new(String::from_utf8_lossy(line.as_bytes()).as_ref())
                }
            }
            Modification::Addition(diff::Addition { line, line_no }) => {
                if let Some(lines) = &blobs.new.as_ref() {
                    lines[*line_no as usize - 1].clone()
                } else {
                    term::Line::new(String::from_utf8_lossy(line.as_bytes()).as_ref())
                }
            }
            Modification::Context {
                line, line_no_new, ..
            } => {
                // Nb. we can check in the old or the new blob, we choose the new.
                if let Some(lines) = &blobs.new.as_ref() {
                    lines[*line_no_new as usize - 1].clone()
                } else {
                    term::Line::new(String::from_utf8_lossy(line.as_bytes()).as_ref())
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::ffi::OsStr;

    use term::Constraint;
    use term::Element;

    use super::*;
    use radicle::git::raw::RepositoryOpenFlags;
    use radicle::git::raw::{Oid, Repository};

    #[test]
    #[ignore]
    fn test_pretty() {
        let repo = Repository::open_ext::<_, _, &[&OsStr]>(
            env!("CARGO_MANIFEST_DIR"),
            RepositoryOpenFlags::all(),
            &[],
        )
        .unwrap();
        let commit = repo
            .find_commit(Oid::from_str("5078396028e2ec5660aa54a00208f6e11df84aa9").unwrap())
            .unwrap();
        let parent = commit.parents().next().unwrap();
        let old_tree = parent.tree().unwrap();
        let new_tree = commit.tree().unwrap();
        let diff = repo
            .diff_tree_to_tree(Some(&old_tree), Some(&new_tree), None)
            .unwrap();
        let diff = Diff::try_from(diff).unwrap();

        let mut hi = Highlighter::default();
        let pretty = diff.pretty(&mut hi, &(), &repo);

        pretty
            .write(Constraint::from_env().unwrap_or_default())
            .unwrap();
    }
}
