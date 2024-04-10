use serde::{Deserialize, Serialize};
use std::io;

/// Program version metadata.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Version<'a> {
    pub name: &'a str,
    pub version: &'a str,
    pub commit: &'a str,
    pub timestamp: &'a str,
}

impl<'a> Version<'a> {
    /// Write program version as string.
    /// Adjust with caution, third party applications parse the string for version info.
    pub fn write(&self, mut w: impl std::io::Write) -> Result<(), io::Error> {
        let Version {
            name,
            version,
            commit,
            ..
        } = self;

        writeln!(w, "{name} {version} ({commit})")?;

        Ok(())
    }

    /// Write the program version metadata as a JSON value.
    pub fn write_json(&self, w: impl std::io::Write) -> Result<(), serde_json::Error> {
        serde_json::to_writer(w, self)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_version() {
        let mut buffer = Vec::new();
        Version {
            name: "rad",
            version: "1.2.3",
            commit: "28b341d",
            timestamp: "",
        }
        .write(&mut buffer)
        .unwrap();
        let res = std::str::from_utf8(&buffer).unwrap();
        assert_eq!("rad 1.2.3 (28b341d)\n", res);
    }
}
