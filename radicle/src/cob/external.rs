//! # External Collaborative Objects
//!
//! This module provides an interface for external helper programs to provide
//! the evaluation logic of Collaborative Objects (COBs).
//!
//! An external COB is one that relies on an external program (so called
//! "helper", is an executable file, for example a script, or a binary), that
//! implements the evaluation logic for that particular COB:
//! Whenever an operation is to be applied to an external COB, the helper is
//! invoked with the current state of the COB and the operation to be applied.
//! It then returns the new state of the COB, according to its internal logic.
//! This concept is borrowed from Git, which supports [remote helpers] and
//! [credential helpers].
//!
//! External COBs must be based on JSON, that is, the COB itself and associated
//! actions must serialize to and deserialize from JSON.
//! Further, the helper must be able to communicate to Radicle using
//! [JSON Lines] (see further details below).
//!
//! # Invocation
//!
//! The helper is invoked by Radicle without command line arguments.
//! In the future, more arguments might be added.
//!
//! # Helper Protocol
//!
//! Radicle and the helper communicate back and forth [JSON Lines] via standard
//! streams.
//!
//!  1. The helper must read and process at least one JSON Line (containing one
//!     operation) from standard input, which represents the operation to be
//!     applied to the COB, along with possible concurrent operations.
//!  2. The helper must write the new state of COB to standard output in a JSON
//!     Line.
//!  3. The helper may read additional JSON Lines from standard input, these are
//!     to be applied "on top of" the previous operations.
//!  4. The helper must reply with the state of the COB in a JSON Line after
//!     processing each operation.
//!  5. The helper must exit with a status code of zero on success, and a
//!     non-zero status code on failure.
//!  6. The helper may write to standard error for logging and debugging
//!     purposes.
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
//! ```
//!
//! [JSON Lines]: https://jsonlines.org/
//! [credential helpers]: https://git-scm.com/doc/credential-helpers
//! [remote helpers]: https://git-scm.com/docs/gitremote-helpers

use std::collections::HashMap;
use std::io::{Error as IoError, ErrorKind};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use serde_json::{from_slice, to_writer, Error as JsonError, Map, Value};

use crate::cob::object::collaboration::Evaluate;
use crate::cob::op::{Op as CobOp, OpEncodingError};
use crate::cob::store::{Cob, CobAction};
use crate::git::Oid;
use crate::storage::ReadRepository;

/// This prefix is used to generate the name of the command,
/// which is executed by the helper to apply operations.
static COB_EXTERNAL_COMMAND_PREFIX: &str = "rad-cob-";

#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub struct External(Value);

impl Default for External {
    fn default() -> Self {
        Self(Value::Object(Map::default()))
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("op decoding failed: {0}")]
    Op(#[from] OpEncodingError),
    #[error("serde_json: {0}")]
    Serde(#[from] JsonError),
    #[error("io: {0}")]
    Io(#[from] IoError),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Action {
    #[serde(default)]
    parents: Vec<Oid>,

    #[serde(flatten)]
    map: HashMap<String, Value>,
}

impl CobAction for Action {
    fn parents(&self) -> Vec<Oid> {
        self.parents.clone()
    }
}

impl From<Action> for nonempty::NonEmpty<Action> {
    fn from(action: Action) -> Self {
        Self::new(action)
    }
}

pub type Op = CobOp<Action>;

impl External {
    fn handle<R: ReadRepository>(
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

        let child = Command::new(command_name)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        let Some(stdin) = &child.stdin else {
            return Err(Error::Io(IoError::new(
                ErrorKind::BrokenPipe,
                "stdin not available",
            )));
        };

        #[derive(Serialize)]
        struct OpMessage {
            value: Value,
            op: Op,
            concurrent: Vec<Op>,
        }

        to_writer(
            stdin,
            &OpMessage {
                value: self.0.clone(),
                op,
                concurrent,
            },
        )?;

        self.0 = from_slice(&child.wait_with_output()?.stdout)?;
        Ok(())
    }
}

impl Cob for External {
    type Action = Action;
    type Error = Error;

    fn from_root<R: ReadRepository>(
        op: super::Op<Self::Action>,
        repo: &R,
    ) -> Result<Self, Self::Error> {
        let mut root = Self::default();
        root.handle(op, vec![], repo)?;
        Ok(root)
    }

    fn op<'a, R: ReadRepository, I: IntoIterator<Item = &'a super::Entry>>(
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

impl<R: ReadRepository> Evaluate<R> for External {
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
