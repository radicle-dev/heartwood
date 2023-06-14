#![allow(clippy::collapsible_else_if)]
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync;
use std::{env, ffi, fs, io, mem};

use snapbox::cmd::{Command, OutputAssert};
use snapbox::{Assert, Substitutions};
use thiserror::Error;

/// Used to ensure the build task is only run once.
static BUILD: sync::Once = sync::Once::new();

#[derive(Error, Debug)]
pub enum Error {
    #[error("parsing failed")]
    Parse,
    #[error("invalid file path: {0:?}")]
    InvalidFilePath(String),
    #[error("test file not found: {0:?}")]
    TestNotFound(PathBuf),
    #[error("i/o: {0}")]
    Io(#[from] io::Error),
    #[error("snapbox: {0}")]
    Snapbox(#[from] snapbox::Error),
}

#[derive(Debug, PartialEq, Eq)]
enum ExitStatus {
    Success,
    Failure,
}

/// A test which may contain multiple assertions.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Test {
    /// Human-readable context around the test. Functions as documentation.
    context: Vec<String>,
    /// Test assertions to run.
    assertions: Vec<Assertion>,
    /// Whether to check stderr's output instead of stdout.
    stderr: bool,
    /// Whether to expect an error status code.
    fail: bool,
    /// Local env vars to use just for this test.
    env: HashMap<String, String>,
}

/// An assertion is a command to run with an expected output.
#[derive(Debug, PartialEq, Eq)]
pub struct Assertion {
    /// The test file that contains this assertion.
    path: PathBuf,
    /// Name of command to run, eg. `git`.
    command: String,
    /// Command arguments, eg. `["push"]`.
    args: Vec<String>,
    /// Expected output (stdout or stderr).
    expected: String,
    /// Expected exit status.
    exit: ExitStatus,
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
    /// Binaries path.
    bins: Vec<PathBuf>,
}

impl TestFormula {
    pub fn new() -> Self {
        Self {
            cwd: PathBuf::new(),
            env: HashMap::new(),
            tests: Vec::new(),
            subs: Substitutions::new(),
            bins: env::var("PATH")
                .map(|p| p.split(':').map(PathBuf::from).collect())
                .unwrap_or_default(),
        }
    }

    pub fn build(&mut self, binaries: &[(&str, &str)]) -> &mut Self {
        let manifest = env::var("CARGO_MANIFEST_DIR").expect(
            "TestFormula::build: cannot build binaries: variable `CARGO_MANIFEST_DIR` is not set",
        );
        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        let target_dir = env::var("CARGO_TARGET_DIR").unwrap_or("target".to_string());
        let manifest = Path::new(manifest.as_str());
        let bins = manifest.join(&target_dir).join(profile);

        // Add the target dir to the beginning of the list we will use as `PATH`.
        self.bins.insert(0, bins);

        // We don't need to re-build everytime the `build` function is called. Once is enough.
        BUILD.call_once(|| {
            use escargot::format::Message;

            for (package, binary) in binaries {
                log::debug!(target: "test", "Building binaries for package `{package}`..");

                let results = escargot::CargoBuild::new()
                    .package(package)
                    .bin(binary)
                    .manifest_path(&manifest.join("Cargo.toml"))
                    .target_dir(&target_dir)
                    .exec()
                    .unwrap();

                for result in results {
                    match result {
                        Ok(msg) => {
                            if let Ok(Message::CompilerArtifact(a)) = msg.decode() {
                                if let Some(e) = a.executable {
                                    log::debug!(target: "test", "Built {}", e.display());
                                }
                            }
                        }
                        Err(e) => {
                            log::error!(target: "test", "Error building package `{package}`: {e}");
                        }
                    }
                }
            }
        });
        self
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
        let path = path.as_ref();
        let contents = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Err(Error::TestNotFound(path.to_path_buf()));
            }
            Err(err) => return Err(err.into()),
        };
        self.read(path, io::Cursor::new(contents))
    }

    pub fn read(&mut self, path: &Path, r: impl io::BufRead) -> Result<&mut Self, Error> {
        let mut test = Test::default();
        let mut fenced = false; // Whether we're inside a fenced code block.
        let mut file: Option<(PathBuf, String)> = None; // Path and content of file created by this test block.

        for line in r.lines() {
            let line = line?;

            if line.starts_with("```") {
                if fenced {
                    if let Some((ref path, ref mut content)) = file.take() {
                        // Write file.
                        let path = self.cwd.join(path);

                        if let Some(dir) = path.parent() {
                            log::debug!(target: "test", "Creating directory {}..", dir.display());
                            fs::create_dir_all(dir)?;
                        }
                        log::debug!(target: "test", "Writing {} bytes to {}..", content.len(), path.display());
                        fs::write(path, content)?;
                    } else {
                        // End existing code block.
                        self.tests.push(mem::take(&mut test));
                    }
                } else {
                    for token in line.split_whitespace() {
                        if token.contains("stderr") {
                            test.stderr = true;
                        }
                        if token.contains("fail") {
                            test.fail = true;
                        }
                        if let Some(path) = token.strip_prefix("./") {
                            file = Some((
                                PathBuf::from_str(path)
                                    .map_err(|_| Error::InvalidFilePath(token.to_owned()))?,
                                String::new(),
                            ));
                        }
                        if let Some((key, val)) = token.split_once('=') {
                            test.env.insert(key.to_owned(), val.to_owned());
                        }
                    }
                }
                fenced = !fenced;

                continue;
            }

            if fenced {
                if let Some((_, ref mut content)) = file {
                    content.push_str(line.as_str());
                    content.push('\n');
                } else if let Some(line) = line.strip_prefix('$') {
                    let line = line.trim();
                    let parts = shlex::split(line).ok_or(Error::Parse)?;
                    let (cmd, args) = parts.split_first().ok_or(Error::Parse)?;

                    test.assertions.push(Assertion {
                        path: path.to_path_buf(),
                        command: cmd.to_owned(),
                        args: args.to_owned(),
                        expected: String::new(),
                        exit: if test.fail {
                            ExitStatus::Failure
                        } else {
                            ExitStatus::Success
                        },
                    });
                } else if let Some(a) = test.assertions.last_mut() {
                    a.expected.push_str(line.as_str());
                    a.expected.push('\n');
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

    /// Convert instances of '[..   ]' to '[..]' where the number of ' 's are arbitrary.
    ///
    /// Supporting these bracket types help support using the '[..]' pattern while preserving
    /// spaces important for text alignment.
    fn map_spaced_brackets(s: &str) -> String {
        let mut ret = String::new();
        let mut pos = 0;

        for c in s.chars() {
            match (c, pos) {
                ('[', 0) => pos += 1,
                (' ', 1) => continue,
                ('.', 1) => pos += 1,
                ('.', 2) => pos += 1,
                ('.', 3) => continue,
                (' ', 3) => continue,
                (']', 3) => pos = 0,
                (_, _) => pos = 0,
            }
            ret.push(c);
        }

        ret
    }

    pub fn run(&mut self) -> Result<bool, io::Error> {
        let assert = Assert::new().substitutions(self.subs.clone());

        fs::create_dir_all(&self.cwd)?;
        log::debug!(target: "test", "Using PATH {:?}", self.bins);

        for test in &self.tests {
            for assertion in &test.assertions {
                let path = assertion
                    .path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or(String::from("<none>"));
                let cmd = if assertion.command == "rad" {
                    snapbox::cmd::cargo_bin("rad")
                } else if assertion.command == "cd" {
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
                    PathBuf::from(&assertion.command)
                };
                log::debug!(target: "test", "{path}: Running `{}` with {:?} in `{}`..", cmd.display(), assertion.args, self.cwd.display());

                if !self.cwd.exists() {
                    log::error!(target: "test", "{path}: Directory {} does not exist..", self.cwd.display());
                }

                let bins = self
                    .bins
                    .iter()
                    .map(|p| p.as_os_str())
                    .collect::<Vec<_>>()
                    .join(ffi::OsStr::new(":"));
                let result = Command::new(cmd.clone())
                    .env_clear()
                    .env("PATH", &bins)
                    .env("RUST_BACKTRACE", "1")
                    .envs(self.env.clone())
                    .envs(test.env.clone())
                    .current_dir(&self.cwd)
                    .args(&assertion.args)
                    .with_assert(assert.clone())
                    .output();

                match result {
                    Ok(output) => {
                        let assert = OutputAssert::new(output).with_assert(assert.clone());
                        let expected = Self::map_spaced_brackets(&assertion.expected);

                        let matches = if test.stderr {
                            assert.stderr_matches(&expected)
                        } else {
                            assert.stdout_matches(&expected)
                        };
                        match assertion.exit {
                            ExitStatus::Success => {
                                matches.success();
                            }
                            ExitStatus::Failure => {
                                matches.failure();
                            }
                        }
                    }
                    Err(err) => {
                        if err.kind() == io::ErrorKind::NotFound {
                            log::error!(target: "test", "{path}: Command `{}` does not exist..", cmd.display());
                        }
                        return Err(io::Error::new(
                            err.kind(),
                            format!("{path}: {err}: `{}`", cmd.display()),
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
        let path = Path::new("test.md").to_path_buf();
        actual
            .read(path.as_path(), io::BufReader::new(io::Cursor::new(input)))
            .unwrap();

        let expected = TestFormula {
            cwd: PathBuf::new(),
            env: HashMap::new(),
            subs: Substitutions::new(),
            bins: env::var("PATH")
                .unwrap()
                .split(':')
                .map(PathBuf::from)
                .collect(),
            tests: vec![
                Test {
                    context: vec![String::from("Let's try to track @dave and @sean:")],
                    assertions: vec![
                        Assertion {
                            path: path.clone(),
                            command: String::from("rad"),
                            args: vec![String::from("track"), String::from("@dave")],
                            expected: String::from(
                                "Tracking relationship established for @dave.\nNothing to do.\n\n",
                            ),
                            exit: ExitStatus::Success,
                        },
                        Assertion {
                            path: path.clone(),
                            command: String::from("rad"),
                            args: vec![String::from("track"), String::from("@sean")],
                            expected: String::from(
                                "Tracking relationship established for @sean.\nNothing to do.\n",
                            ),
                            exit: ExitStatus::Success,
                        },
                    ],
                    fail: false,
                    stderr: false,
                    env: HashMap::default(),
                },
                Test {
                    context: vec![String::from("Super, now let's move on to the next step.")],
                    assertions: vec![Assertion {
                        path: path.clone(),
                        command: String::from("rad"),
                        args: vec![String::from("sync")],
                        expected: String::new(),
                        exit: ExitStatus::Success,
                    }],
                    fail: false,
                    stderr: false,
                    env: HashMap::default(),
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
name = "radicle-cli-test"
```
"#
        .trim()
        .as_bytes()
        .to_owned();

        let mut formula = TestFormula::new();
        formula
            .cwd(env!("CARGO_MANIFEST_DIR"))
            .read(
                Path::new("test.md"),
                io::BufReader::new(io::Cursor::new(input)),
            )
            .unwrap();
        formula.run().unwrap();
    }

    #[test]
    fn test_example_spaced_brackets() {
        let input = r#"
Running a simple command such as `head`:
```
$ echo "    hello"
[..]hello
$ echo "    hello"
[..  ]hello
$ echo "    hello"
[  ..]hello
$ echo "[bug, good-first-issue]"
[bug, good-first-issue]
$ echo "[bug, good-first-issue]"
[bug, [  ..    ]-issue]
$ echo "[bug, good-first-issue]"
[bug, [  ...   ]-issue]
```
"#
        .trim()
        .as_bytes()
        .to_owned();

        let mut formula = TestFormula::new();
        formula
            .cwd(env!("CARGO_MANIFEST_DIR"))
            .read(
                Path::new("test.md"),
                io::BufReader::new(io::Cursor::new(input)),
            )
            .unwrap();
        formula.run().unwrap();
    }
}
