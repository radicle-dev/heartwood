#[cfg(feature = "serde")]
mod branch;
mod code_browsing;
#[cfg(feature = "serde")]
mod commit;
mod diff;
mod file_system;
mod r#gen;
mod last_commit;
mod namespace;
mod platinum;
mod reference;
mod repository;
mod rev;
#[cfg(feature = "serde")]
mod roundtrip;
#[cfg(feature = "serde")]
mod source;
mod submodule;
mod threading;
