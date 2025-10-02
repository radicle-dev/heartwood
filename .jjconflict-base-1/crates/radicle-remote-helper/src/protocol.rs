use thiserror::Error;

#[derive(Debug, Error)]
pub(super) enum Error {
    #[error("invalid command `{0}`")]
    InvalidCommand(String),
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Command {
    Capabilities,
    List,
    ListForPush,
    Fetch { oid: String, refstr: String },
    Push(String),
    Option { key: String, value: Option<String> },
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Line {
    Valid(Command),
    Blank,
}

impl Command {
    pub(super) fn parse_line(line: &str) -> Result<Line, Error> {
        let line = line.trim();
        if line.is_empty() {
            return Ok(Line::Blank);
        }

        // Split the command verb from the rest of the line.
        let (cmd, args) = line.split_once(' ').unwrap_or((line, ""));
        let args = args.trim();

        match cmd {
            "capabilities" => Ok(Line::Valid(Command::Capabilities)),
            "list" => {
                if args == "for-push" {
                    Ok(Line::Valid(Command::ListForPush))
                } else if args.is_empty() {
                    Ok(Line::Valid(Command::List))
                } else {
                    Err(Error::InvalidCommand(line.to_owned()))
                }
            }
            "fetch" => {
                // fetch <oid> <name>
                // Use split_whitespace to handle multiple spaces between OID and Ref,
                // which is permitted.
                let mut parts = args.split_whitespace();
                let oid = parts
                    .next()
                    .ok_or_else(|| Error::InvalidCommand(line.to_owned()))?;
                let refstr = parts
                    .next()
                    .ok_or_else(|| Error::InvalidCommand(line.to_owned()))?;
                Ok(Line::Valid(Command::Fetch {
                    oid: oid.to_owned(),
                    refstr: refstr.to_owned(),
                }))
            }
            "push" => Ok(Line::Valid(Command::Push(args.to_owned()))),
            "option" => {
                // option <key> [value]
                // Use split_once to preserve whitespace in the value.
                let (key, val) = args.split_once(' ').unwrap_or((args, ""));
                let value = if val.is_empty() {
                    None
                } else {
                    Some(val.to_owned())
                };
                Ok(Line::Valid(Command::Option {
                    key: key.to_owned(),
                    value,
                }))
            }
            _ => Err(Error::InvalidCommand(line.to_owned())),
        }
    }
}

mod io {
    use std::io::{self, prelude::*};

    use super::*;

    pub(crate) struct LineReader<R: Read> {
        inner: io::BufReader<R>,
    }

    impl<R: Read> LineReader<R> {
        pub(crate) fn new(reader: R) -> Self {
            Self {
                inner: io::BufReader::new(reader),
            }
        }

        pub(crate) fn read_line(&mut self) -> io::Result<Result<Line, Error>> {
            let mut line = String::new();
            if self.inner.read_line(&mut line)? == 0 {
                // EOF reached
                return Ok(Ok(Line::Blank));
            }
            Ok(Command::parse_line(&line))
        }
    }

    impl<R: Read> Iterator for LineReader<R> {
        type Item = io::Result<Result<Line, Error>>;

        fn next(&mut self) -> Option<Self::Item> {
            match self.read_line() {
                Ok(line) => Some(Ok(line)),
                Err(e) => Some(Err(e)),
            }
        }
    }
}

pub(crate) use io::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capabilities() {
        assert_eq!(
            Command::parse_line("capabilities").unwrap(),
            Line::Valid(Command::Capabilities)
        );
    }

    #[test]
    fn test_list() {
        assert_eq!(
            Command::parse_line("list").unwrap(),
            Line::Valid(Command::List)
        );
    }

    #[test]
    fn test_list_for_push() {
        assert_eq!(
            Command::parse_line("list for-push").unwrap(),
            Line::Valid(Command::ListForPush)
        );
    }

    #[test]
    fn test_fetch() {
        assert_eq!(
            Command::parse_line("fetch oid ref").unwrap(),
            Line::Valid(Command::Fetch {
                oid: "oid".to_owned(),
                refstr: "ref".to_owned()
            })
        );
    }

    #[test]
    fn test_fetch_whitespace() {
        assert_eq!(
            Command::parse_line("fetch   oid     ref").unwrap(),
            Line::Valid(Command::Fetch {
                oid: "oid".to_owned(),
                refstr: "ref".to_owned()
            })
        );
    }

    #[test]
    fn test_push() {
        assert_eq!(
            Command::parse_line("push src:dst").unwrap(),
            Line::Valid(Command::Push("src:dst".to_owned()))
        );
    }

    #[test]
    fn test_push_force() {
        assert_eq!(
            Command::parse_line("push +src:dst").unwrap(),
            Line::Valid(Command::Push("+src:dst".to_owned()))
        );
    }

    #[test]
    fn test_push_delete() {
        assert_eq!(
            Command::parse_line("push :dst").unwrap(),
            Line::Valid(Command::Push(":dst".to_owned()))
        );
    }

    #[test]
    fn test_option() {
        assert_eq!(
            Command::parse_line("option verbosity 2").unwrap(),
            Line::Valid(Command::Option {
                key: "verbosity".to_owned(),
                value: Some("2".to_owned())
            })
        );
    }

    #[test]
    fn test_option_whitespace_preservation() {
        assert_eq!(
            Command::parse_line("option patch.message Fix:  whitespace").unwrap(),
            Line::Valid(Command::Option {
                key: "patch.message".to_owned(),
                value: Some("Fix:  whitespace".to_owned())
            })
        );
    }

    #[test]
    fn test_empty() {
        assert_eq!(Command::parse_line("").unwrap(), Line::Blank);
        assert_eq!(Command::parse_line("   ").unwrap(), Line::Blank);
    }

    #[test]
    fn test_invalid() {
        assert!(Command::parse_line("invalid command").is_err());
        assert!(Command::parse_line("list invalid").is_err());
    }
}
