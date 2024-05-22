//! # External Collaborative Objects
//!
//! This module provides an interface for external helper programs to provide the evaluation logic of Collaborative Objects (COBs).
//!
//! An external COB is one that relies on an external program (so called "helper", is an executable file, for example a script, or a binary), that implements the evaluation logic for that particular COB:
//! Whenever an operation is to be applied to an external COB, the helper is invoked with the current state of the COB and the operation to be applied.
//! It then returns the new state of the COB, according to its internal logic.
//! This concept is borrowed from Git, which supports [remote helpers] and [credential helpers].
//!
//! External COBs must be based on JSON, that is, the COB itself and associated actions must serialize to and deserialize from JSON.
//! Further, the helper must be able to communicate to Radicle using [JSON Lines] (see further details below).
//!
//! # Invocation
//!
//! The helper is invoked by Radicle without command line arguments.
//! In the future, more arguments might be added.
//!
//! # Helper Protocol
//!
//! Radicle and the helper communicate back and forth [JSON Lines] via standard streams.
//!
//!  1. The helper must read and process at least one JSON Line (containing one operation) from standard input, which represents the operation to be applied to the COB, along with possible concurrent operations.
//!  2. The helper must write the new state of COB to standard output in a JSON Line.
//!  3. The helper may read additional JSON Lines from standard input, these are to be applied "on top of" the previous operations.
//!  4. The helper must reply with the state of the COB in a JSON Line after processing each operation.
//!  5. The helper must exit with a status code of zero on success, and a non-zero status code on failure.
//!  6. The helper may write to standard error for logging and debugging purposes.
//!
//! # Syntax of Operations
//!
//! The operations sent from Radicle to the helper are of the following shape:
//!
//! ```json
//! {
//!     "title": "Operation as sent to helper"
//!     "type": "object",
//!     "properties": {
//!         "concurrent": {
//!             "type": "array",
//!             "items": {
//!                 "$ref": "#/definitions/radicle::cob::Op"
//!             }
//!         },
//!         "value": {
//!             "type": "object",
//!             "properties": {
//!                 "prop": {
//!                     "$ref": "#/definitions/radicle::cob::external::External"
//!                 }
//!             }
//!         },
//!         "op": {
//!             "type": "object",
//!             "properties": {
//!                 "prop": {
//!                     "$ref": "#/definitions/radicle::cob::Op"
//!                 }
//!             }
//!         }
//!     },
//! }
//! ````
//!
//! [JSON Lines]: https://jsonlines.org/
//! [credential helpers]: https://git-scm.com/doc/credential-helpers
//! [remote helpers]: https://git-scm.com/docs/gitremote-helpers

use std::collections::HashMap;
use std::process;

use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use serde_json::Value;

use crate::cob::store::Cob;
use crate::git;
use crate::storage::ReadRepository;

/// This prefix is used to generate the name of the command,
/// which is executed by the helper to apply operations.
static COB_EXTERNAL_COMMAND_PREFIX: &str = "rad-cob-";

#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub struct External(serde_json::Value);

impl Default for External {
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
pub struct Action {
    #[serde(default)]
    parents: Vec<git::Oid>,

    #[serde(flatten)]
    map: HashMap<String, Value>,
}

impl super::store::CobAction for Action {
    fn parents(&self) -> Vec<git::Oid> {
        self.parents.clone()
    }
}

impl From<Action> for nonempty::NonEmpty<Action> {
    fn from(action: Action) -> Self {
        Self::new(action)
    }
}

pub type Op = crate::cob::Op<Action>;

impl External {
    fn handle<'a, R: crate::test::storage::ReadRepository>(
        &mut self,
        op: Op,
        concurrent: Vec<Op>,
        _repo: &R,
    ) -> Result<(), Error> {
        let command_name = {
            let prefix = String::from(COB_EXTERNAL_COMMAND_PREFIX);
            let type_name = op.manifest.type_name.to_string();
            let suffix = type_name
                .rsplit_once('.')
                .map(|(_, suffix)| suffix)
                .unwrap_or(type_name.as_str());
            prefix + suffix
        };

        let child = process::Command::new(command_name)
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped())
            .spawn()?;

        if let Some(stdin) = &child.stdin {
            serde_json::to_writer(
                stdin,
                &json!({
                    "value": self.0.clone(),
                    "op": op,
                    "concurrent": concurrent,
                }),
            )?;

            self.0 = serde_json::from_slice(&child.wait_with_output()?.stdout)?;
            Ok(())
        } else {
            Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "stdin not available",
            )))
        }
    }
}

impl Cob for External {
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

impl<R: ReadRepository> crate::cob::Evaluate<R> for External {
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
