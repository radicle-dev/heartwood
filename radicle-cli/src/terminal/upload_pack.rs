use std::collections::BTreeSet;
use std::time::Instant;

use crate::terminal::format;
use radicle::node::NodeId;

/// Keeps track of upload-pack progress for displaying to the terminal.
pub struct UploadPack {
    /// Keep track of which remotes are being uploaded to, removing any that
    /// have completed.
    remotes: BTreeSet<NodeId>,
    /// Keep track of how long we've been transmitting for to calculate
    /// throughput.
    timer: Instant,
}

impl Default for UploadPack {
    fn default() -> Self {
        Self::new()
    }
}

impl UploadPack {
    /// Construct an empty set of spinners.
    pub fn new() -> Self {
        Self {
            remotes: BTreeSet::new(),
            timer: Instant::now(),
        }
    }

    /// Display the number of peers, the total transmitted bytes, and the
    /// throughput.
    pub fn transmitted(&mut self, remote: NodeId, transmitted: usize) -> String {
        self.remotes.insert(remote);
        let throughput = transmitted as f64 / self.timer.elapsed().as_secs_f64();
        let throughput = format::bytes(throughput.floor() as usize);
        let n = self.remotes.len();
        let transmitted = format::bytes(transmitted);
        format!("Uploading to {n} peers ({transmitted} | {throughput:.2}/s)")
    }

    /// Display which remote has completed upload-pack and how many are
    /// remaining.
    pub fn done(&mut self, remote: &NodeId) -> String {
        self.remotes.remove(remote);
        let n = self.remotes.len();
        format!("Uploaded to {remote}, {n} peers remaining..")
    }
}
