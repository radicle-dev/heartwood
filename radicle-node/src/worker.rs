use crossbeam_channel as chan;
use netservices::noise::NoiseXk;
use netservices::wire::NetTransport;

use radicle::crypto::Negotiator;

use crate::prelude::Message;
use crate::service::reactor::Fetch;
use crate::service::FetchResult;

pub struct WorkerReq<G: Negotiator> {
    pub fetch: Fetch,
    pub session: NetTransport<NoiseXk<G>, Message>,
    pub channel: chan::Sender<WorkerResp<G>>,
}

pub struct WorkerResp<G: Negotiator> {
    pub result: FetchResult,
    pub session: NetTransport<NoiseXk<G>, Message>,
}
