pub mod project;
pub use project::{Project, RemoteProject};

pub mod person;
pub use person::Person;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Urn {
    pub name: Name,
    pub remote: Option<Name>,
}

impl Urn {
    pub fn to_path(&self) -> String {
        match &self.remote {
            Some(remote) => format!("{}/{}", self.name.as_str(), remote.as_str()),
            None => self.name.0.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Name(String);

impl Name {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
