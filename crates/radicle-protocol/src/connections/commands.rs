use localtime::LocalTime;
use radicle::{
    node::{Link, NodeId},
    prelude::RepoId,
};

use crate::service::message;

pub enum Command {
    Attempt(Attempt),
    Connect(Connect),
    Disconnect(Disconnect),
    Subscribe(Subscribe),
    SubscribeTo(SubscribeTo),
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

impl From<Subscribe> for Command {
    fn from(v: Subscribe) -> Self {
        Self::Subscribe(v)
    }
}

impl From<SubscribeTo> for Command {
    fn from(v: SubscribeTo) -> Self {
        Self::SubscribeTo(v)
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subscribe {
    pub node: NodeId,
    pub subscription: message::Subscribe,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubscribeTo {
    pub node: NodeId,
    pub rid: RepoId,
}
