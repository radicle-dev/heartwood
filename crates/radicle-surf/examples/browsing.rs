//! An example of browsing a git repo using `radicle-surf`.
//!
//! How to run:
//!
//!     cargo run --example browsing <git_repo_path>
//!
//! This program browses the given repo and prints out the files and
//! the directories in a tree-like structure.

use radicle_surf::{
    fs::{self, Directory},
    Repository,
};
use std::{env, time::Instant};

fn main() {
    let repo_path = match env::args().nth(1) {
        Some(path) => path,
        None => {
            print_usage();
            return;
        }
    };
    let repo = Repository::discover(repo_path).unwrap();
    let now = Instant::now();
    let head = repo.head().unwrap();
    let root = repo.root_dir(head).unwrap();
    print_directory(&root, &repo, 0);

    let elapsed_millis = now.elapsed().as_millis();
    println!("browse with print: {elapsed_millis} ms");
}

fn print_directory(d: &Directory, repo: &Repository, indent_level: usize) {
    let indent = " ".repeat(indent_level * 4);
    println!("{}{}/", &indent, d.name());
    for entry in d.entries(repo).unwrap() {
        match entry {
            fs::Entry::File(f) => println!("    {}{}", &indent, f.name()),
            fs::Entry::Directory(d) => print_directory(&d, repo, indent_level + 1),
            fs::Entry::Submodule(s) => println!("    {}{}", &indent, s.name()),
        }
    }
}

fn print_usage() {
    println!("Usage:");
    println!("cargo run --example browsing <repo_path>");
}
