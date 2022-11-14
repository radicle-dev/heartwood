pub trait Transcode {}

#[derive(Debug, Default)]
pub struct PlainTranscoder;

impl Transcode for PlainTranscoder {}
