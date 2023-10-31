use radicle_cli::terminal as term;
use radicle_httpd::commands::web as rad_web;

fn main() {
    term::run_command_args::<rad_web::Options, _>(
        rad_web::HELP,
        rad_web::run,
        std::env::args_os().skip(1).collect(),
    )
}
