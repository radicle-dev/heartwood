#![allow(clippy::collapsible_else_if)]
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::{env, fs, io, mem};

use snapbox::cmd::{Command, OutputAssert};
use snapbox::{Assert, Substitutions};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("parsing failed")]
    Parse,
    #[error("i/o: {0}")]
    Io(#[from] io::Error),
    #[error("snapbox: {0}")]
    Snapbox(#[from] snapbox::Error),
}

/// A test which may contain multiple assertions.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Test {
    /// Human-readable context around the test. Functions as documentation.
    context: Vec<String>,
    /// Test assertions to run.
    assertions: Vec<Assertion>,
}

/// An assertion is a command to run with an expected output.
#[derive(Debug, PartialEq, Eq)]
pub struct Assertion {
    /// Name of program to run, eg. `git`.
    program: String,
    /// Program arguments, eg. `["push"]`.
    args: Vec<String>,
    /// Expected output (stdout or stderr).
    expected: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct TestFormula {
    /// Current working directory to run the test in.
    cwd: PathBuf,
    /// Environment to pass to the test.
    env: HashMap<String, String>,
    /// Tests to run.
    tests: Vec<Test>,
    /// Output substitutions.
    subs: Substitutions,
}

impl TestFormula {
    pub fn new() -> Self {
        Self {
            cwd: PathBuf::new(),
            env: HashMap::new(),
            tests: Vec::new(),
            subs: Substitutions::new(),
        }
    }

    pub fn cwd(&mut self, path: impl AsRef<Path>) -> &mut Self {
        self.cwd = path.as_ref().into();
        self
    }

    pub fn env(&mut self, key: impl Into<String>, val: impl Into<String>) -> &mut Self {
        self.env.insert(key.into(), val.into());
        self
    }

    pub fn envs<K: ToString, V: ToString>(
        &mut self,
        envs: impl IntoIterator<Item = (K, V)>,
    ) -> &mut Self {
        for (k, v) in envs {
            self.env.insert(k.to_string(), v.to_string());
        }
        self
    }

    pub fn file(&mut self, path: impl AsRef<Path>) -> Result<&mut Self, Error> {
        let contents = fs::read(path)?;
        self.read(io::Cursor::new(contents))
    }

    pub fn read(&mut self, r: impl io::BufRead) -> Result<&mut Self, Error> {
        let mut test = Test::default();
        let mut fenced = false; // Whether we're inside a fenced code block.

        for line in r.lines() {
            let line = line?;

            if line.starts_with("```") {
                if fenced {
                    // End existing code block.
                    self.tests.push(mem::take(&mut test));
                }
                fenced = !fenced;

                continue;
            }

            if fenced {
                if let Some(line) = line.strip_prefix('$') {
                    let line = line.trim();
                    let parts = shlex::split(line).ok_or(Error::Parse)?;
                    let (program, args) = parts.split_first().ok_or(Error::Parse)?;

                    test.assertions.push(Assertion {
                        program: program.to_owned(),
                        args: args.to_owned(),
                        expected: String::new(),
                    });
                } else if let Some(test) = test.assertions.last_mut() {
                    test.expected.push_str(line.as_str());
                    test.expected.push('\n');
                } else {
                    return Err(Error::Parse);
                }
            } else {
                test.context.push(line);
            }
        }
        Ok(self)
    }

    #[allow(dead_code)]
    pub fn substitute(
        &mut self,
        value: &'static str,
        other: impl Into<Cow<'static, str>>,
    ) -> Result<&mut Self, Error> {
        self.subs.insert(value, other)?;
        Ok(self)
    }

    pub fn run(&mut self) -> Result<bool, io::Error> {
        let assert = Assert::new().substitutions(self.subs.clone());

        fs::create_dir_all(&self.cwd)?;

        for test in &self.tests {
            for assertion in &test.assertions {
                let program = if assertion.program == "rad" {
                    snapbox::cmd::cargo_bin("rad")
                } else if assertion.program == "cd" {
                    let path: PathBuf = assertion.args.first().unwrap().into();
                    let path = self.cwd.join(path);

                    // TODO: Add support for `..` and `/`
                    // TODO: Error if more than one args are given.

                    if !path.exists() {
                        return Err(io::Error::new(
                            io::ErrorKind::NotFound,
                            format!("cd: '{}' does not exist", path.display()),
                        ));
                    }
                    self.cwd = path;

                    continue;
                } else {
                    PathBuf::from(&assertion.program)
                };
                log::debug!(target: "test", "Running `{}` in `{}`..", program.display(), self.cwd.display());

                if !program.exists() {
                    log::error!(target: "test", "Program {} does not exist..", program.display());
                }
                if !self.cwd.exists() {
                    log::error!(target: "test", "Directory {} does not exist..", self.cwd.display());
                }
                let result = Command::new(program.clone())
                    .env_clear()
                    .envs(env::vars().filter(|(k, _)| k == "PATH"))
                    .envs(self.env.clone())
                    .current_dir(&self.cwd)
                    .args(&assertion.args)
                    .with_assert(assert.clone())
                    .output();

                match result {
                    Ok(output) => {
                        let assert = OutputAssert::new(output).with_assert(assert.clone());
                        assert.stdout_matches(&assertion.expected).success();
                    }
                    Err(err) => {
                        return Err(io::Error::new(
                            err.kind(),
                            format!("{err}: `{}`", program.display()),
                        ));
                    }
                }
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use pretty_assertions::assert_eq;

    #[test]
    fn test_parse() {
        let input = r#"
Let's try to track @dave and @sean:
```
$ rad track @dave
Tracking relationship established for @dave.
Nothing to do.

$ rad track @sean
Tracking relationship established for @sean.
Nothing to do.
```
Super, now let's move on to the next step.
```
$ rad sync
```
"#
        .trim()
        .as_bytes()
        .to_owned();

        let mut actual = TestFormula::new();
        actual
            .read(io::BufReader::new(io::Cursor::new(input)))
            .unwrap();

        let expected = TestFormula {
            cwd: PathBuf::new(),
            env: HashMap::new(),
            subs: Substitutions::new(),
            tests: vec![
                Test {
                    context: vec![String::from("Let's try to track @dave and @sean:")],
                    assertions: vec![
                        Assertion {
                            program: String::from("rad"),
                            args: vec![String::from("track"), String::from("@dave")],
                            expected: String::from(
                                "Tracking relationship established for @dave.\nNothing to do.\n\n",
                            ),
                        },
                        Assertion {
                            program: String::from("rad"),
                            args: vec![String::from("track"), String::from("@sean")],
                            expected: String::from(
                                "Tracking relationship established for @sean.\nNothing to do.\n",
                            ),
                        },
                    ],
                },
                Test {
                    context: vec![String::from("Super, now let's move on to the next step.")],
                    assertions: vec![Assertion {
                        program: String::from("rad"),
                        args: vec![String::from("sync")],
                        expected: String::new(),
                    }],
                },
            ],
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_run() {
        let input = r#"
Running a simple command such as `head`:
```
$ head -n 2 Cargo.toml
[package]
name = "radicle-cli"
```
"#
        .trim()
        .as_bytes()
        .to_owned();

        let mut formula = TestFormula::new();
        formula
            .cwd(env!("CARGO_MANIFEST_DIR"))
            .read(io::BufReader::new(io::Cursor::new(input)))
            .unwrap();
        formula.run().unwrap();
    }
}
