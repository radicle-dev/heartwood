use serde::{Deserialize, Serialize};

/// The options to be used to calculate the diff for a given `Patch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    /// The algorithm used to calculate the diff.
    ///
    /// Default: [`Algorithm::Histogram`].
    algorithm: Algorithm,
    /// Disable updating the binary flag in delta records.
    ///
    /// Default: `false`.
    skip_binary: bool,
    /// Set the number of unchanged lines that define the boundary of a hunk
    /// (and to display before and after).
    ///
    /// Default: `3`.
    context_lines: u32,
    /// Set the maximum number of unchanged lines between hunk boundaries before
    /// the hunks will be merged into one.
    ///
    /// Default: `0`.
    interhunk_lines: u32,
    /// Configuration options for finding similar files in a diff.
    find: FindOptions,
}

impl Options {
    pub fn new() -> Self {
        Self::default()
    }

    /// The algorithm used to calculate the diff.
    ///
    /// Default: [`Algorithm::Histogram`].
    pub fn with_algorithm(self, algorithm: Algorithm) -> Self {
        Self { algorithm, ..self }
    }

    /// Disable updating the binary flag in delta records.
    ///
    /// Default: `false`.
    pub fn skip_binary(self, skip: bool) -> Self {
        Self {
            skip_binary: skip,
            ..self
        }
    }

    /// Set the number of unchanged lines that define the boundary of a hunk
    /// (and to display before and after).
    ///
    /// Default: `3`.
    pub fn with_context_lines(self, lines: u32) -> Self {
        Self {
            context_lines: lines,
            ..self
        }
    }

    /// Set the maximum number of unchanged lines between hunk boundaries before
    /// the hunks will be merged into one.
    ///
    /// Default: `0`.
    pub fn with_interhunk_lines(self, lines: u32) -> Self {
        Self {
            interhunk_lines: lines,
            ..self
        }
    }

    /// Construct the [`git2::DiffOptions`] using the [`Options`] provided.
    ///
    /// The following flags are ensured to be not set:
    ///
    ///  - `skip_binary_check`
    ///  - `force_text`
    ///  - `force_binary`
    fn as_diff_options(&self) -> git2::DiffOptions {
        let mut opts = git2::DiffOptions::new();
        // Options that we want to ensure are not set
        opts.reverse(false).force_text(false).force_binary(false);

        opts.skip_binary_check(self.skip_binary)
            .context_lines(self.context_lines)
            .interhunk_lines(self.interhunk_lines);
        match self.algorithm {
            Algorithm::Myers => opts.patience(false),
            Algorithm::Patience => opts.patience(true),
            Algorithm::Histogram => opts.patience(true).minimal(true),
        };
        opts
    }
}

impl From<&Options> for git2::DiffOptions {
    fn from(opts: &Options) -> Self {
        opts.as_diff_options()
    }
}

impl From<&Options> for git2::DiffFindOptions {
    fn from(opts: &Options) -> Self {
        opts.find.as_diff_find_options()
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            algorithm: Algorithm::default(),
            skip_binary: false,
            context_lines: 3,
            interhunk_lines: 0,
            find: FindOptions::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Algorithm {
    /// The myers algorithm, this is the default used in `git`.
    Myers,
    /// The patience algorithm.
    Patience,
    /// The patience algorithm, which takes slightly more time to minimize the
    /// diff. This is the protocol's default.
    #[default]
    Histogram,
}

/// The options to be used to calculate to find file similarity within a diff.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FindOptions {
    /// Measure similarity only by comparing SHAs (fast and cheap).
    exact_match: bool,
    /// Look for copies.
    #[serde(skip_serializing_if = "Option::is_none")]
    copies: Option<Copies>,
    /// Look for renames.
    #[serde(skip_serializing_if = "Option::is_none")]
    renames: Option<Renames>,
}

impl Default for FindOptions {
    fn default() -> Self {
        Self {
            exact_match: false,
            copies: None,
            renames: Some(Renames::default()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Copies {
    /// Similarity to consider a file copy
    ///
    /// Default: `50`
    pub copy_threshold: u16,
}

impl Default for Copies {
    fn default() -> Self {
        Self { copy_threshold: 50 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Renames {
    /// The number of files to consider when performing the copy/rename detection;
    /// equivalent to the git diff option -l. This setting has no effect if rename
    /// detection is turned off.
    ///
    /// Default: `200`.
    pub limit: usize,
    /// Similarity to consider a file renamed
    ///
    /// Default: `50`.
    pub rename_threshold: u16,
}

impl Default for Renames {
    fn default() -> Self {
        Self {
            limit: 200,
            rename_threshold: 50,
        }
    }
}

impl FindOptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Measure similarity only by comparing SHAs (fast and cheap).
    ///
    /// Default: `true`.
    pub fn with_exact_match(self, exact_match: bool) -> Self {
        Self {
            exact_match,
            ..self
        }
    }

    /// Look for copies.
    ///
    /// Default: `None`
    pub fn with_copies(self, copies: Option<Copies>) -> Self {
        Self { copies, ..self }
    }

    /// Look for renames.
    ///
    /// Default: `Some(Renames::default())`
    pub fn with_renames(self, renames: Option<Renames>) -> Self {
        Self { renames, ..self }
    }

    /// Construct the [`git2::DiffFindOptions`] using the [`FindOptions`] provided.
    pub fn as_diff_find_options(&self) -> git2::DiffFindOptions {
        let mut opts = git2::DiffFindOptions::new();
        match self.renames {
            Some(Renames {
                limit,
                rename_threshold,
            }) => {
                opts.renames(true);
                opts.rename_limit(limit);
                opts.rename_from_rewrite_threshold(rename_threshold);
            }
            None => {
                opts.renames(false);
            }
        }
        match self.copies {
            Some(Copies { copy_threshold }) => {
                opts.copies(true);
                opts.copy_threshold(copy_threshold);
            }
            None => {
                opts.copies(false);
            }
        }
        opts.exact_match_only(self.exact_match);
        opts
    }
}

impl From<&FindOptions> for git2::DiffFindOptions {
    fn from(opts: &FindOptions) -> Self {
        opts.as_diff_find_options()
    }
}
