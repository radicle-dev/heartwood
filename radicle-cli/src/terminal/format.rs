use std::{fmt, time};

pub use radicle_term::format::*;
pub use radicle_term::{style, Paint};

use radicle::cob::{ObjectId, Timestamp};
use radicle::identity::Visibility;
use radicle::node::{Alias, AliasStore, NodeId};
use radicle::prelude::Did;
use radicle::profile::Profile;
use radicle_term::element::Line;

use crate::terminal as term;

/// Format a node id to be more compact.
pub fn node(node: &NodeId) -> Paint<String> {
    let node = node.to_human();
    let start = node.chars().take(7).collect::<String>();
    let end = node.chars().skip(node.len() - 7).collect::<String>();

    Paint::new(format!("{start}…{end}"))
}

/// Format a git Oid.
pub fn oid(oid: impl Into<radicle::git::Oid>) -> Paint<String> {
    Paint::new(format!("{:.7}", oid.into()))
}

/// Wrap parenthesis around styled input, eg. `"input"` -> `"(input)"`.
pub fn parens<D: fmt::Display>(input: Paint<D>) -> Paint<String> {
    Paint::new(format!("({})", input.item)).with_style(input.style)
}

/// Wrap spaces around styled input, eg. `"input"` -> `" input "`.
pub fn spaced<D: fmt::Display>(input: Paint<D>) -> Paint<String> {
    Paint::new(format!(" {} ", input.item)).with_style(input.style)
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

/// Format a Visibility.
pub fn visibility(v: &Visibility) -> Paint<&str> {
    match v {
        Visibility::Public => term::format::positive("public"),
        Visibility::Private { .. } => term::format::secondary("private"),
    }
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
            true => self::node(nid).to_string(),
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
pub struct Author<'a> {
    nid: &'a NodeId,
    alias: Option<Alias>,
    you: bool,
}

impl<'a> Author<'a> {
    pub fn new(nid: &'a NodeId, profile: &Profile) -> Author<'a> {
        let alias = profile.aliases().alias(nid);

        Self {
            nid,
            alias,
            you: nid == profile.id(),
        }
    }

    pub fn labels(self) -> (term::Label, term::Label) {
        let alias = match &self.alias {
            Some(alias) => term::format::primary(alias).into(),
            None => term::format::primary(term::format::node(self.nid))
                .dim()
                .into(),
        };
        if self.you {
            (alias, term::format::primary("(you)").dim().italic().into())
        } else {
            (
                alias,
                term::format::primary(term::format::node(self.nid))
                    .dim()
                    .into(),
            )
        }
    }

    pub fn line(self) -> Line {
        let (first, second) = self.labels();
        Line::spaced([first, second])
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
