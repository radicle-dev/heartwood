use crate::encoding::{Buffer, Cursor};

pub trait Public: Sized {
    type Error;

    fn write_blob(&self, buf: &mut Buffer) -> usize;
    fn read(reader: &mut Cursor) -> Result<Option<Self>, Self::Error>;
}

pub trait Private: Sized {
    type Error;

    fn read(reader: &mut Cursor) -> Result<Option<(Vec<u8>, Self)>, Self::Error>;
    fn write(&self, buf: &mut Buffer) -> Result<(), Self::Error>;
    fn write_signature<T: AsRef<[u8]>>(
        &self,
        buf: &mut Buffer,
        to_sign: T,
    ) -> Result<(), Self::Error>;
}
