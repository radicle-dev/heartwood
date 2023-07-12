use std::{fmt, time};

pub use radicle_term::format::*;
pub use radicle_term::{style, Paint};

use radicle::cob::{ObjectId, Timestamp};
use radicle::node::{Alias, AliasStore, NodeId};
use radicle::prelude::Did;
use radicle::profile::Profile;
use radicle_term::element::Line;

use crate::terminal as term;

/// Format a node id to be more compact.
pub fn node(node: &NodeId) -> String {
    let node = node.to_human();
    let start = node.chars().take(7).collect::<String>();
    let end = node.chars().skip(node.len() - 7).collect::<String>();

    format!("{start}…{end}")
}

/// Format a git Oid.
pub fn oid(oid: impl Into<radicle::git::Oid>) -> Paint<String> {
    Paint::new(format!("{:.7}", oid.into()))
}

/// Wrap parenthesis around styled input, eg. `"input"` -> `"(input)"`.
pub fn parens<D: fmt::Display>(input: Paint<D>) -> Paint<String> {
    Paint::new(format!("({})", input.item)).with_style(input.style)
}

/// Format a command suggestion, eg. `rad init`.
pub fn command<D: fmt::Display>(cmd: D) -> Paint<String> {
    primary(format!("`{cmd}`"))
}

/// Format a COB id.
pub fn cob(id: &ObjectId) -> String {
    format!("{:.7}", id.to_string())
}

/// Format a DID.
pub fn did(did: &Did) -> Paint<String> {
    let nid = did.as_key().to_human();
    Paint::new(format!("{}…{}", &nid[..7], &nid[nid.len() - 7..]))
}

/// Remove html style comments from a string.
///
/// The html comments must start at the beginning of a line and stop at the end.
pub fn strip_comments(s: &str) -> String {
    let ends_with_newline = s.ends_with('\n');
    let mut is_comment = false;
    let mut w = String::new();

    for line in s.lines() {
        if is_comment {
            if line.ends_with("-->") {
                is_comment = false;
            }
            continue;
        } else if line.starts_with("<!--") {
            is_comment = true;
            continue;
        }

        w.push_str(line);
        w.push('\n');
    }
    if !ends_with_newline {
        w.pop();
    }

    w.to_string()
}

/// Format a timestamp.
pub fn timestamp(time: &Timestamp) -> Paint<String> {
    let fmt = timeago::Formatter::new();
    let now = Timestamp::now();
    let duration = time::Duration::from_secs(now.as_secs() - time.as_secs());

    Paint::new(fmt.convert(duration))
}

/// Identity formatter that takes a profile and displays it as
/// `<node-id> (<username>)` depending on the configuration.
pub struct Identity<'a> {
    profile: &'a Profile,
    /// If true, node id is printed in its compact form.
    short: bool,
    /// If true, node id and username are printed using the terminal's
    /// styled formatters.
    styled: bool,
}

impl<'a> Identity<'a> {
    pub fn new(profile: &'a Profile) -> Self {
        Self {
            profile,
            short: false,
            styled: false,
        }
    }

    pub fn short(mut self) -> Self {
        self.short = true;
        self
    }

    pub fn styled(mut self) -> Self {
        self.styled = true;
        self
    }
}

impl<'a> fmt::Display for Identity<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let nid = self.profile.id();
        let alias = self.profile.aliases().alias(nid);
        let node_id = match self.short {
            true => self::node(nid),
            false => nid.to_human(),
        };

        if self.styled {
            write!(f, "{}", term::format::highlight(node_id))?;
            if let Some(a) = alias {
                write!(f, " {}", term::format::parens(term::format::dim(a)))?;
            }
        } else {
            write!(f, "{node_id}")?;
            if let Some(a) = alias {
                write!(f, " ({a})")?;
            }
        }
        Ok(())
    }
}

/// This enum renders (nid, alias) in terminal depending on user variant.
pub enum Author<'a> {
    Author {
        nid: &'a NodeId,
        alias: Option<Alias>,
    },
    Me {
        alias: Option<Alias>,
    },
}

impl<'a> Author<'a> {
    pub fn new(nid: &'a NodeId, alias: Option<Alias>, me: &Profile) -> Author<'a> {
        if nid == me.id() {
            Self::Me { alias }
        } else {
            Self::Author { nid, alias }
        }
    }

    /// Author: `<alias>` || ``
    /// Me    : `<alias> (you)` || `(you)`
    pub fn alias(&self) -> Line {
        match self {
            Self::Me { alias } => {
                if let Some(alias) = alias {
                    term::Line::spaced([
                        term::format::primary(alias).into(),
                        term::format::primary("(you)").dim().into(),
                    ])
                } else {
                    term::format::primary("(you)").into()
                }
            }

            Self::Author { alias, .. } => {
                if let Some(alias) = alias {
                    term::format::primary(alias).into()
                } else {
                    term::format::default(String::new()).into()
                }
            }
        }
    }
}

impl<'a> IntoIterator for Author<'a> {
    type Item = term::Label;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    /// Author : `<alias> (<compact-nid>)` || `<nid>`
    /// Me     : `<alias> (you)` || `(you)`
    fn into_iter(self) -> Self::IntoIter {
        let mut line = Vec::new();

        match self {
            Self::Me { alias } => {
                if let Some(alias) = alias {
                    line.push(term::format::primary(alias).into());
                    line.push(term::Label::space());
                    line.push(term::format::primary("(you)").dim().into());
                } else {
                    line.push(term::format::primary("(you)").into());
                }
            }

            Self::Author { nid, alias } => {
                if let Some(alias) = alias {
                    line.push(term::format::primary(alias).into());
                    line.push(term::Label::space());
                    line.push(
                        term::format::tertiary(term::format::parens(
                            term::format::node(nid).into(),
                        ))
                        .into(),
                    );
                } else {
                    line.push(term::format::tertiary(nid).into());
                }
            }
        }

        line.into_iter()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_strip_comments() {
        let test = "\
        commit 2\n\
        \n\
        <!--\n\
        Please enter a comment for your patch update. Leaving this\n\
        blank is also okay.\n\
        -->";
        let exp = "\
        commit 2\n\
        ";

        let res = strip_comments(test);
        assert_eq!(exp, res);

        let test = "\
        commit 2\n\
        -->";
        let exp = "\
        commit 2\n\
        -->";

        let res = strip_comments(test);
        assert_eq!(exp, res);

        let test = "\
        <!--\n\
        commit 2\n\
        ";
        let exp = "";

        let res = strip_comments(test);
        assert_eq!(exp, res);

        let test = "\
        commit 2\n\
        \n\
        <!--\n\
        <!--\n\
        Please enter a comment for your patch update. Leaving this\n\
        blank is also okay.\n\
        -->\n\
        -->";
        let exp = "\
        commit 2\n\
        \n\
        -->";

        let res = strip_comments(test);
        assert_eq!(exp, res);
    }
}
