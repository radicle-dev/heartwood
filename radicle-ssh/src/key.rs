use std::error::Error;

use crate::encoding::{Buffer, Cursor};

/// A public SSH key.
pub trait Public: Sized {
    type Error: Error + Send + Sync + 'static;

    /// Write the public key to the given buffer, in SSH "blob" format.
    fn write(&self, buf: &mut Buffer) -> usize;
    /// Read the public key from the given reader.
    fn read(reader: &mut Cursor) -> Result<Option<Self>, Self::Error>;
}

/// A private SSH key.
pub trait Private: Sized {
    type Error: Error + Send + Sync + 'static;

    /// Read a private key from the given reader.
    fn read(reader: &mut Cursor) -> Result<Option<Self>, Self::Error>;
    /// Write the key bytes to the supplied buffer.
    fn write(&self, buf: &mut Buffer) -> Result<(), Self::Error>;
    /// Sign the data and write the signature to the given buffer.
    fn write_signature<T: AsRef<[u8]>>(&self, data: T, buf: &mut Buffer)
        -> Result<(), Self::Error>;
}
