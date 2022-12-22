use crossbeam_channel as chan;
use netservices::noise::NoiseXk;

use radicle::crypto::Negotiator;

use crate::service::reactor::Fetch;
use crate::service::FetchResult;

/// Worker request.
pub struct WorkerReq<G: Negotiator> {
    pub fetch: Fetch,
    pub session: NoiseXk<G>,
    pub drain: Vec<u8>,
    pub channel: chan::Sender<WorkerResp<G>>,
}

/// Worker response.
pub struct WorkerResp<G: Negotiator> {
    pub result: FetchResult,
    pub session: NoiseXk<G>,
}
