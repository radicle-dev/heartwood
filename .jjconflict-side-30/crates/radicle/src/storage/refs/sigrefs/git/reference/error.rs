use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
#[error(transparent)]
pub struct FindReference(Box<dyn std::error::Error + Send + Sync + 'static>);

impl FindReference {
    pub fn other<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self(Box::new(err))
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
#[error(transparent)]
pub struct WriteReference(Box<dyn std::error::Error + Send + Sync + 'static>);

impl WriteReference {
    pub fn other<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self(Box::new(err))
    }
}
