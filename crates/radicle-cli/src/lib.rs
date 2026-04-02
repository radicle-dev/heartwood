#![allow(clippy::collapsible_if)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::too_many_arguments)]
// Prevent use of `println!` and `print!` which panic on broken pipes.
// Use `term::print()` or `term::print_inline()` instead.
// See: https://github.com/rust-lang/rust/issues/62569
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
