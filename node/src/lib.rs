#![allow(dead_code)]
pub use nakamoto_net::{Io, Link, LocalDuration, LocalTime};

mod address_book;
mod address_manager;
mod clock;
mod collections;
mod crypto;
mod decoder;
mod git;
mod hash;
mod identity;
mod logger;
mod protocol;
mod rad;
mod storage;
#[cfg(test)]
mod test;

pub fn run() -> anyhow::Result<()> {
    Ok(())
}
