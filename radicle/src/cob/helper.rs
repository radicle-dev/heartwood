//! Generic COBs via helpers.
//! A helper represents an executable file, for example a (shell) script,
//! or a binary, that implements its evaluation logic.

use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::Value;
use tempfile::{NamedTempFile, TempDir};
use thiserror::Error;

use crate::cob::store::Cob;
use crate::storage::ReadRepository;

/// This prefix is used to generate the name of the command,
/// which is executed by the helper to apply operations.
static COB_HELPER_COMMAND_PREFIX: &str = "rad-cob-";

#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub struct Helper(serde_json::Value);

impl Default for Helper {
    fn default() -> Self {
        Self(json!({}))
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("op decoding failed: {0}")]
    Op(#[from] super::op::OpEncodingError),
    #[error("serde_json: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Action(serde_json::Value);

impl super::store::CobAction for Action {}

impl From<Action> for nonempty::NonEmpty<Action> {
    fn from(action: Action) -> Self {
        Self::new(action)
    }
}

pub type Op = crate::cob::Op<Action>;

impl Helper {
    fn handle<'a, R: crate::test::storage::ReadRepository>(
        &mut self,
        op: Op,
        concurrent: Vec<Op>,
        _repo: &R,
    ) -> Result<(), Error> {
        let op_ser: Value = {
            let mut ser = json!(op);
            ser.as_object_mut()
                .expect("ops must serialize to objects")
                .insert(
                    "actions".to_string(),
                    json!(op
                        .actions
                        .iter()
                        .map(|action: &Action| -> Result<Value, _> {
                            serde_json::from_value(action.0.clone())
                        })
                        .collect::<Result<Vec<Value>, _>>()?),
                );
            ser
        };

        let command_name = {
            let prefix = String::from(COB_HELPER_COMMAND_PREFIX);
            let type_name = op.manifest.type_name.to_string();
            let suffix = type_name
                .rsplit_once('.')
                .map(|(_, suffix)| suffix)
                .unwrap_or(type_name.as_str());
            prefix + suffix
        };

        let tmp_dir = TempDir::new()?;

        let self_file = NamedTempFile::with_prefix_in("self-", &tmp_dir)?;
        serde_json::to_writer(&self_file, &self)?;

        let op_file = NamedTempFile::with_prefix_in("op-", &tmp_dir)?;
        serde_json::to_writer(&op_file, &op_ser)?;

        let concurrent_files: Vec<NamedTempFile> = concurrent
            .into_iter()
            .enumerate()
            .map(|(i, op)| -> Result<NamedTempFile, Error> {
                let concurrent_file =
                    NamedTempFile::with_prefix_in(i.to_string() + "-concurrent-", &tmp_dir)?;
                serde_json::to_writer(&concurrent_file, &op)?;
                Ok(concurrent_file)
            })
            .collect::<Result<Vec<NamedTempFile>, _>>()?;

        let mut cmd = std::process::Command::new(command_name);
        cmd.arg(self_file.path());
        cmd.arg(op_file.path());

        for concurrent_file in concurrent_files.iter() {
            cmd.arg(concurrent_file.path());
        }

        self.0 = serde_json::from_slice(&cmd.output()?.stdout)?;
        Ok(())
    }
}

impl Cob for Helper {
    type Action = Action;

    type Error = Error;

    fn from_root<R: crate::test::storage::ReadRepository>(
        op: super::Op<Self::Action>,
        repo: &R,
    ) -> Result<Self, Self::Error> {
        let mut root = Self::default();
        root.handle(op, vec![], repo)?;
        Ok(root)
    }

    fn op<'a, R: crate::test::storage::ReadRepository, I: IntoIterator<Item = &'a super::Entry>>(
        &mut self,
        op: super::Op<Self::Action>,
        concurrent: I,
        repo: &R,
    ) -> Result<(), <Self as Cob>::Error> {
        let concurrent: Vec<Op> = concurrent
            .into_iter()
            .map(Op::try_from)
            .collect::<Result<Vec<Op>, _>>()?;
        self.handle(op, concurrent, repo)
    }
}

impl<R: ReadRepository> crate::cob::Evaluate<R> for Helper {
    type Error = Error;

    fn init(entry: &radicle_cob::Entry, store: &R) -> Result<Self, Self::Error> {
        Self::from_root(Op::try_from(entry)?, store)
    }

    fn apply<'a, I: Iterator<Item = (&'a radicle_git_ext::Oid, &'a radicle_cob::Entry)>>(
        &mut self,
        entry: &radicle_cob::Entry,
        concurrent: I,
        repo: &R,
    ) -> Result<(), Self::Error> {
        let concurrent: Vec<Op> = concurrent
            .map(|(_, e)| e)
            .map(Op::try_from)
            .collect::<Result<Vec<Op>, _>>()?;
        self.handle(Op::try_from(entry)?, concurrent, repo)
    }
}
