#![allow(clippy::too_many_arguments)]
// Prevent use of `println!` and `print!` which panic on broken pipes.
// Use `terminal::io::println` or `terminal::io::print` instead.
#![deny(clippy::print_stdout)]
pub mod commands;
pub mod git;
pub mod node;
pub mod pager;
pub mod project;
pub mod terminal;

mod common_args;
mod warning;

extern crate radicle_localtime as localtime;
