use localtime::LocalTime;
use radicle::node::{Link, NodeId};

pub enum Command {
    Attempt(Attempt),
    Connect(Connect),
    Disconnect(Disconnect),
}

impl From<Attempt> for Command {
    fn from(v: Attempt) -> Self {
        Self::Attempt(v)
    }
}

impl From<Connect> for Command {
    fn from(v: Connect) -> Self {
        Self::Connect(v)
    }
}

impl From<Disconnect> for Command {
    fn from(v: Disconnect) -> Self {
        Self::Disconnect(v)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attempt {
    pub node: NodeId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Connect {
    pub node: NodeId,
    pub now: LocalTime,
    pub link: Link,
    pub persistent: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Disconnect {
    pub node: NodeId,
    pub since: LocalTime,
    pub retry_at: Option<LocalTime>,
}
