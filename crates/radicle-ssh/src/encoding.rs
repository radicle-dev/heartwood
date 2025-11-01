// Copyright 2016 Pierre-Étienne Meunier
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
use std::ops::DerefMut as _;

use thiserror::Error;
use zeroize::Zeroizing;

/// General purpose writable byte buffer we use everywhere.
pub type Buffer = Zeroizing<Vec<u8>>;

#[derive(Debug, Error)]
pub enum Error {
    /// Index out of bounds
    #[error("Index out of bounds")]
    IndexOutOfBounds,
}

pub trait Encodable: Sized {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Read from the SSH format.
    fn read(reader: &mut Cursor) -> Result<Self, Self::Error>;
    /// Write to the SSH format.
    fn write<E: Encoding>(&self, buf: &mut E);
}

/// Encode in the SSH format.
pub trait Encoding {
    /// Push an SSH-encoded string to `self`.
    fn extend_ssh_string(&mut self, s: &[u8]);
    /// Push an SSH-encoded blank string of length `s` to `self`.
    fn extend_ssh_string_blank(&mut self, s: usize) -> &mut [u8];
    /// Push an SSH-encoded multiple-precision integer.
    fn extend_ssh_mpint(&mut self, s: &[u8]);
    /// Push an SSH-encoded list.
    fn extend_list<'a, I: Iterator<Item = &'a [u8]>>(&mut self, list: I);
    /// Push an SSH-encoded unsigned 32-bit integer.
    fn extend_u32(&mut self, u: u32);
    /// Push an SSH-encoded empty list.
    fn write_empty_list(&mut self);
    /// Write the buffer length at the beginning of the buffer.
    fn write_len(&mut self);
    /// Push a [`usize`] as an SSH-encoded unsigned 32-bit integer.
    /// May panic if the argument is greater than [`u32::MAX`].
    /// This is a convenience method, to spare callers casting or converting
    /// [`usize`] to [`u32`]. If callers end up in a situation where they
    /// need to push a 32-bit unsigned integer, but the value they would
    /// like to push does not fit 32 bits, then the implementation will not
    /// comply with the SSH format anyway.
    fn extend_usize(&mut self, u: usize) {
        self.extend_u32(u.try_into().unwrap())
    }
}

/// Encoding length of the given mpint.
pub fn mpint_len(s: &[u8]) -> usize {
    let mut i = 0;
    while i < s.len() && s[i] == 0 {
        i += 1
    }
    (if s[i] & 0x80 != 0 { 5 } else { 4 }) + s.len() - i
}

impl Encoding for Vec<u8> {
    fn extend_ssh_string(&mut self, s: &[u8]) {
        self.extend_usize(s.len());
        self.extend(s);
    }

    fn extend_ssh_string_blank(&mut self, len: usize) -> &mut [u8] {
        self.extend_usize(len);
        let current = self.len();
        self.resize(current + len, 0u8);

        &mut self[current..]
    }

    fn extend_ssh_mpint(&mut self, s: &[u8]) {
        // Skip initial 0s.
        let mut i = 0;
        while i < s.len() && s[i] == 0 {
            i += 1
        }
        // If the first non-zero is >= 128, write its length (u32, BE), followed by 0.
        if s[i] & 0x80 != 0 {
            self.extend_usize(s.len() - i + 1);
            self.push(0)
        } else {
            self.extend_usize(s.len() - i);
        }
        self.extend(&s[i..]);
    }

    fn extend_u32(&mut self, s: u32) {
        self.extend(s.to_be_bytes());
    }

    fn extend_list<'a, I: Iterator<Item = &'a [u8]>>(&mut self, list: I) {
        let len0 = self.len();

        let mut first = true;
        for i in list {
            if !first {
                self.push(b',')
            } else {
                first = false;
            }
            self.extend(i)
        }
        let len = (self.len() - len0 - 4) as u32;

        self.splice(len0..len0, len.to_be_bytes());
    }

    fn write_empty_list(&mut self) {
        self.extend([0, 0, 0, 0]);
    }

    fn write_len(&mut self) {
        let len = self.len() - 4;
        self[..4].copy_from_slice((len as u32).to_be_bytes().as_slice());
    }
}

impl Encoding for Buffer {
    fn extend_ssh_string(&mut self, s: &[u8]) {
        self.deref_mut().extend_ssh_string(s)
    }

    fn extend_ssh_string_blank(&mut self, len: usize) -> &mut [u8] {
        self.deref_mut().extend_ssh_string_blank(len)
    }

    fn extend_ssh_mpint(&mut self, s: &[u8]) {
        self.deref_mut().extend_ssh_mpint(s)
    }

    fn extend_list<'a, I: Iterator<Item = &'a [u8]>>(&mut self, list: I) {
        self.deref_mut().extend_list(list)
    }

    fn write_empty_list(&mut self) {
        self.deref_mut().write_empty_list()
    }

    fn extend_u32(&mut self, s: u32) {
        self.deref_mut().extend_u32(s);
    }

    fn write_len(&mut self) {
        self.deref_mut().write_len()
    }
}

/// A cursor-like trait to read SSH-encoded things.
pub trait Reader {
    /// Create an SSH reader for `self`.
    fn reader(&self, starting_at: usize) -> Cursor<'_>;
}

impl Reader for Buffer {
    fn reader(&self, starting_at: usize) -> Cursor<'_> {
        Cursor {
            s: self,
            position: starting_at,
        }
    }
}

impl Reader for [u8] {
    fn reader(&self, starting_at: usize) -> Cursor<'_> {
        Cursor {
            s: self,
            position: starting_at,
        }
    }
}

/// A cursor-like type to read SSH-encoded values.
#[derive(Debug)]
pub struct Cursor<'a> {
    s: &'a [u8],
    #[doc(hidden)]
    pub position: usize,
}

impl<'a> Cursor<'a> {
    /// Read one string from this reader.
    pub fn read_string(&mut self) -> Result<&'a [u8], Error> {
        let len = self.read_u32()? as usize;
        if self.position + len <= self.s.len() {
            let result = &self.s[self.position..(self.position + len)];
            self.position += len;
            Ok(result)
        } else {
            Err(Error::IndexOutOfBounds)
        }
    }

    /// Read a `u32` from this reader.
    pub fn read_u32(&mut self) -> Result<u32, Error> {
        if self.position + 4 <= self.s.len() {
            let u =
                u32::from_be_bytes(self.s[self.position..self.position + 4].try_into().unwrap());
            self.position += 4;
            Ok(u)
        } else {
            Err(Error::IndexOutOfBounds)
        }
    }

    /// Read one byte from this reader.
    pub fn read_byte(&mut self) -> Result<u8, Error> {
        if self.position < self.s.len() {
            let u = self.s[self.position];
            self.position += 1;
            Ok(u)
        } else {
            Err(Error::IndexOutOfBounds)
        }
    }

    pub fn read_bytes<const S: usize>(&mut self) -> Result<[u8; S], Error> {
        let mut buf = [0; S];
        for b in buf.iter_mut() {
            *b = self.read_byte()?;
        }
        Ok(buf)
    }

    /// Read one byte from this reader.
    pub fn read_mpint(&mut self) -> Result<&'a [u8], Error> {
        let len = self.read_u32()? as usize;
        if self.position + len <= self.s.len() {
            let result = &self.s[self.position..(self.position + len)];
            self.position += len;
            Ok(result)
        } else {
            Err(Error::IndexOutOfBounds)
        }
    }
}
