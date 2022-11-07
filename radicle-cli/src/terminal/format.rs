use std::fmt;

use dialoguer::console::style;
use radicle::node::NodeId;
use radicle::profile::Profile;
use radicle_cob::ObjectId;

use crate::terminal as term;

/// Format a node id to be more compact.
pub fn node(node: &NodeId) -> String {
    let node = node.to_human();
    let start = node.chars().take(7).collect::<String>();
    let end = node.chars().skip(node.len() - 7).collect::<String>();

    format!("{}…{}", start, end)
}

/// Format a git Oid.
pub fn oid(oid: &radicle::git::Oid) -> String {
    format!("{:.7}", oid)
}

/// Format a COB id.
pub fn cob(id: &ObjectId) -> String {
    format!("{:.11}", id.to_string())
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
        let username = "(me)";
        let node_id = match self.short {
            true => self::node(self.profile.id()),
            false => self.profile.id().to_human(),
        };

        if self.styled {
            write!(
                f,
                "{} {}",
                term::format::highlight(node_id),
                term::format::dim(username)
            )
        } else {
            write!(f, "{} {}", node_id, username)
        }
    }
}

pub fn negative<D: std::fmt::Display>(msg: D) -> String {
    style(msg).red().bright().to_string()
}

pub fn positive<D: std::fmt::Display>(msg: D) -> String {
    style(msg).green().bright().to_string()
}

pub fn secondary<D: std::fmt::Display>(msg: D) -> String {
    style(msg).blue().bright().to_string()
}

pub fn tertiary<D: std::fmt::Display>(msg: D) -> String {
    style(msg).cyan().to_string()
}

pub fn tertiary_bold<D: std::fmt::Display>(msg: D) -> String {
    style(msg).cyan().bold().to_string()
}

pub fn yellow<D: std::fmt::Display>(msg: D) -> String {
    style(msg).yellow().to_string()
}

pub fn highlight<D: std::fmt::Display>(input: D) -> String {
    style(input).green().bright().to_string()
}

pub fn badge_primary<D: std::fmt::Display>(input: D) -> String {
    style(format!(" {} ", input))
        .magenta()
        .reverse()
        .to_string()
}

pub fn badge_positive<D: std::fmt::Display>(input: D) -> String {
    style(format!(" {} ", input)).green().reverse().to_string()
}

pub fn badge_negative<D: std::fmt::Display>(input: D) -> String {
    style(format!(" {} ", input)).red().reverse().to_string()
}

pub fn badge_secondary<D: std::fmt::Display>(input: D) -> String {
    style(format!(" {} ", input)).blue().reverse().to_string()
}

pub fn bold<D: std::fmt::Display>(input: D) -> String {
    style(input).white().bright().bold().to_string()
}

pub fn dim<D: std::fmt::Display>(input: D) -> String {
    style(input).dim().to_string()
}

pub fn italic<D: std::fmt::Display>(input: D) -> String {
    style(input).italic().dim().to_string()
}
