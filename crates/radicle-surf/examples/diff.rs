extern crate radicle_surf;

use std::{env::Args, str::FromStr, time::Instant};

use radicle_git_ext::Oid;
use radicle_surf::{diff::Diff, Repository};

fn main() {
    let options = get_options_or_exit();
    let repo = init_repository_or_exit(&options.path_to_repo);
    let head_oid = match options.head_revision {
        HeadRevision::Head => repo.head().unwrap(),
        HeadRevision::Commit(id) => Oid::from_str(&id).unwrap(),
    };
    let base_oid = Oid::from_str(&options.base_revision).unwrap();
    let now = Instant::now();
    let elapsed_nanos = now.elapsed().as_nanos();
    let diff = repo.diff(base_oid, head_oid).unwrap();
    print_diff_summary(&diff, elapsed_nanos);
}

fn get_options_or_exit() -> Options {
    match Options::parse(std::env::args()) {
        Ok(options) => options,
        Err(message) => {
            println!("{message}");
            std::process::exit(1);
        }
    }
}

fn init_repository_or_exit(path_to_repo: &str) -> Repository {
    match Repository::open(path_to_repo) {
        Ok(repo) => repo,
        Err(e) => {
            println!("Failed to create repository: {e:?}");
            std::process::exit(1);
        }
    }
}

fn print_diff_summary(diff: &Diff, elapsed_nanos: u128) {
    diff.added().for_each(|created| {
        println!("+++ {:?}", created.path);
    });
    diff.deleted().for_each(|deleted| {
        println!("--- {:?}", deleted.path);
    });
    diff.modified().for_each(|modified| {
        println!("mod {:?}", modified.path);
    });

    println!(
        "created {} / deleted {} / modified {} / total {}",
        diff.added().count(),
        diff.deleted().count(),
        diff.modified().count(),
        diff.added().count() + diff.deleted().count() + diff.modified().count()
    );
    println!("diff took {elapsed_nanos} nanos ");
}

struct Options {
    path_to_repo: String,
    base_revision: String,
    head_revision: HeadRevision,
}

enum HeadRevision {
    Head,
    Commit(String),
}

impl Options {
    fn parse(args: Args) -> Result<Self, String> {
        let args: Vec<String> = args.collect();
        if args.len() != 4 {
            return Err(format!(
                "Usage: {} <path-to-repo> <base-revision> <head-revision>\n\
                \tpath-to-repo: Path to the directory containing .git subdirectory\n\
                \tbase-revision: Git commit ID of the base revision (one that will be considered less recent)\n\
                \thead-revision: Git commit ID of the head revision (one that will be considered more recent) or 'HEAD' to use current git HEAD\n",
                args[0]));
        }

        let path_to_repo = args[1].clone();
        let base_revision = args[2].clone();
        let head_revision = {
            if args[3].eq_ignore_ascii_case("HEAD") {
                HeadRevision::Head
            } else {
                HeadRevision::Commit(args[3].clone())
            }
        };

        Ok(Options {
            path_to_repo,
            base_revision,
            head_revision,
        })
    }
}
