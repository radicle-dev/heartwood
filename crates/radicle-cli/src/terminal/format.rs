use std::fmt;

use localtime::LocalTime;

pub use radicle_term::format::*;
pub use radicle_term::{style, Paint};

use radicle::cob::ObjectId;
use radicle::identity::Visibility;
use radicle::node::policy::Policy;
use radicle::node::{Address, Alias, AliasStore, HostName, NodeId};
use radicle::prelude::Did;
use radicle::profile::{env, Profile};
use radicle::storage::RefUpdate;
use radicle_term::element::Line;

use crate::terminal as term;

/// Format a node id to be more compact.
#[must_use]
pub fn node_id_human_compact(node: &NodeId) -> Paint<String> {
    let node = node.to_human();
    let start = node.chars().take(7).collect::<String>();
    let end = node.chars().skip(node.len() - 7).collect::<String>();

    Paint::new(format!("{start}…{end}"))
}

/// Format a node id.
#[must_use]
pub fn node_id_human(node: &NodeId) -> Paint<String> {
    Paint::new(node.to_human())
}

#[must_use]
pub fn addr_compact(address: &Address) -> Paint<String> {
    let host = match address.host() {
        HostName::Ip(ip) => ip.to_string(),
        HostName::Dns(dns) => dns.clone(),
        HostName::Tor(onion) => {
            let onion = onion.to_string();
            let start = onion.chars().take(8).collect::<String>();
            let end = onion
                .chars()
                .skip(onion.len() - 8 - ".onion".len())
                .collect::<String>();
            format!("{start}…{end}")
        }
        _ => unreachable!(),
    };

    let port = address.port().to_string();

    Paint::new(format!("{host}:{port}"))
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
#[must_use]
pub fn cob(id: &ObjectId) -> Paint<String> {
    Paint::new(format!("{:.7}", id.to_string()))
}

/// Format a DID.
#[must_use]
pub fn did(did: &Did) -> Paint<String> {
    let nid = did.as_key().to_human();
    Paint::new(format!("{}…{}", &nid[..7], &nid[nid.len() - 7..]))
}

/// Format a Visibility.
#[must_use]
pub fn visibility(v: &Visibility) -> Paint<&str> {
    match v {
        Visibility::Public => term::format::positive("public"),
        Visibility::Private { .. } => term::format::yellow("private"),
    }
}

/// Format a policy.
#[must_use]
pub fn policy(p: &Policy) -> Paint<String> {
    match p {
        Policy::Allow => term::format::positive(p.to_string()),
        Policy::Block => term::format::negative(p.to_string()),
    }
}

/// Format a timestamp.
pub fn timestamp(time: impl Into<LocalTime>) -> Paint<String> {
    let time = time.into();
    let now = env::local_time();
    let duration = now - time;
    let fmt = timeago::Formatter::new();

    Paint::new(fmt.convert(duration.into()))
}

#[must_use]
pub fn bytes(size: usize) -> Paint<String> {
    const KB: usize = 1024;
    const MB: usize = 1024usize.pow(2);
    const GB: usize = 1024usize.pow(3);
    let size = if size < KB {
        format!("{size} B")
    } else if size < MB {
        format!("{} KiB", size / KB)
    } else if size < GB {
        format!("{} MiB", size / MB)
    } else {
        format!("{} GiB", size / GB)
    };
    Paint::new(size)
}

/// Format a ref update.
#[must_use]
pub fn ref_update(update: &RefUpdate) -> Paint<&'static str> {
    match update {
        RefUpdate::Updated { .. } => term::format::tertiary("updated"),
        RefUpdate::Created { .. } => term::format::positive("created"),
        RefUpdate::Deleted { .. } => term::format::negative("deleted"),
        RefUpdate::Skipped { .. } => term::format::dim("skipped"),
    }
}

#[must_use]
pub fn ref_update_verbose(update: &RefUpdate) -> Paint<String> {
    match update {
        RefUpdate::Created { name, .. } => format!(
            "{: <17} {}",
            term::format::positive("* [new ref]"),
            term::format::secondary(name),
        )
        .into(),
        RefUpdate::Updated { name, old, new } => format!(
            "{: <17} {}",
            format!("{}..{}", term::format::oid(*old), term::format::oid(*new)),
            term::format::secondary(name),
        )
        .into(),
        RefUpdate::Deleted { name, .. } => format!(
            "{: <17} {}",
            term::format::negative("- [deleted]"),
            term::format::secondary(name),
        )
        .into(),
        RefUpdate::Skipped { name, .. } => format!(
            "{: <17} {}",
            term::format::italic("* [skipped]"),
            term::format::secondary(name)
        )
        .into(),
    }
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
    #[must_use]
    pub fn new(profile: &'a Profile) -> Self {
        Self {
            profile,
            short: false,
            styled: false,
        }
    }

    #[must_use]
    pub fn short(mut self) -> Self {
        self.short = true;
        self
    }

    #[must_use]
    pub fn styled(mut self) -> Self {
        self.styled = true;
        self
    }
}

impl fmt::Display for Identity<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let nid = self.profile.id();
        let alias = self.profile.aliases().alias(nid);
        let node_id = match self.short {
            true => self::node_id_human_compact(nid).to_string(),
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
    verbose: bool,
}

impl<'a> Author<'a> {
    #[must_use]
    pub fn new(nid: &'a NodeId, profile: &Profile, verbose: bool) -> Author<'a> {
        let alias = profile.alias(nid);

        Self {
            nid,
            alias,
            you: nid == profile.id(),
            verbose,
        }
    }

    #[must_use]
    pub fn alias(&self) -> Option<term::Label> {
        self.alias.as_ref().map(|a| a.to_string().into())
    }

    #[must_use]
    pub fn you(&self) -> Option<term::Label> {
        if self.you {
            Some(term::format::primary("(you)").dim().italic().into())
        } else {
            None
        }
    }

    /// Get the labels of the `Author`. The labels can take the following forms:
    ///
    ///   * `(<alias>, (you))` -- the `Author` is the local peer and has an alias
    ///   * `(<did>, (you))` -- the `Author` is the local peer and has no alias
    ///   * `(<alias>, <did>)` -- the `Author` is another peer and has an alias
    ///   * `(<blank>, <did>)` -- the `Author` is another peer and has no alias
    #[must_use]
    pub fn labels(self) -> (term::Label, term::Label) {
        let node_id = if self.verbose {
            term::format::node_id_human(self.nid)
        } else {
            term::format::node_id_human_compact(self.nid)
        };

        let alias = match self.alias.as_ref() {
            Some(alias) => term::format::primary(alias).into(),
            None if self.you => term::format::primary(node_id.clone()).dim().into(),
            None => term::Label::blank(),
        };
        let author = self
            .you()
            .unwrap_or_else(move || term::format::primary(node_id).dim().into());
        (alias, author)
    }

    #[must_use]
    pub fn line(self) -> Line {
        let (alias, author) = self.labels();
        Line::spaced([alias, author])
    }
}

/// HTML-related formatting.
pub mod html {
    /// Comment a string with HTML comments.
    #[must_use]
    pub fn commented(s: &str) -> String {
        format!("<!--\n{s}\n-->")
    }

    /// Remove html style comments from a string.
    ///
    /// The HTML comments must start at the beginning of a line and stop at the end.
    #[must_use]
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
}

/// Issue formatting
pub mod issue {
    use super::*;
    use radicle::issue::{CloseReason, State};

    /// Format issue state.
    #[must_use]
    pub fn state(s: &State) -> term::Paint<String> {
        match s {
            State::Open => term::format::positive(s.to_string()),
            State::Closed {
                reason: CloseReason::Other,
            } => term::format::negative(s.to_string()),
            State::Closed {
                reason: CloseReason::Solved,
            } => term::format::secondary(s.to_string()),
        }
    }
}

/// Patch formatting
pub mod patch {
    use super::*;
    use radicle::patch::{State, Verdict};

    #[must_use]
    pub fn verdict(v: Option<Verdict>) -> term::Paint<String> {
        match v {
            Some(Verdict::Accept) => term::PREFIX_SUCCESS.into(),
            Some(Verdict::Reject) => term::PREFIX_ERROR.into(),
            None => term::format::dim("-".to_string()),
        }
    }

    /// Format patch state.
    #[must_use]
    pub fn state(s: &State) -> term::Paint<String> {
        match s {
            State::Draft => term::format::dim(s.to_string()),
            State::Open { .. } => term::format::positive(s.to_string()),
            State::Archived => term::format::yellow(s.to_string()),
            State::Merged { .. } => term::format::secondary(s.to_string()),
        }
    }
}

/// Identity formatting
pub mod identity {
    use super::*;
    use radicle::cob::identity::State;

    /// Format identity revision state.
    #[must_use]
    pub fn state(s: &State) -> term::Paint<String> {
        match s {
            State::Active => term::format::tertiary(s.to_string()),
            State::Accepted => term::format::positive(s.to_string()),
            State::Rejected => term::format::negative(s.to_string()),
            State::Stale => term::format::dim(s.to_string()),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use html::strip_comments;

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

    #[test]
    fn test_bytes() {
        assert_eq!(bytes(1023).to_string(), "1023 B");
        assert_eq!(bytes(1024).to_string(), "1 KiB");
        assert_eq!(bytes(1024 * 9).to_string(), "9 KiB");
        assert_eq!(bytes(1024usize.pow(2)).to_string(), "1 MiB");
        assert_eq!(bytes(1024usize.pow(2) * 56).to_string(), "56 MiB");
        assert_eq!(bytes(1024usize.pow(3) * 1024).to_string(), "1024 GiB");
    }
}
