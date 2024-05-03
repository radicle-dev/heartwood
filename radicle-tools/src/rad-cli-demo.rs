use std::{thread, time};

use radicle_cli::terminal;

fn main() -> anyhow::Result<()> {
    let demo = terminal::io::select(
        "Choose something to try out:",
        &[
            "confirm",
            "pager",
            "spinner",
            "spinner-drop",
            "spinner-error",
            "editor",
            "prompt",
        ],
        "Choose wisely!",
    )?;

    match *demo {
        "confirm" => {
            if terminal::confirm("Would you like to proceed?") {
                terminal::success!("You said 'yes'");
            }
        }
        "pager" => {
            let mut table = radicle_term::Table::<1, radicle_term::Label>::new(
                radicle_term::TableOptions::bordered(),
            );
            let rows = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/rad-cli-demo.rs"));

            for row in rows.lines() {
                table.push([row.into()]);
            }
            radicle_term::pager::page(table)?;
        }
        "editor" => {
            let output = terminal::editor::Editor::new()
                .extension("rs")
                .edit("// Enter code here.");

            match output {
                Ok(Some(s)) => {
                    terminal::info!("You entered:");
                    terminal::blob(s);
                }
                Ok(None) => {
                    terminal::info!("You didn't enter anything.");
                }
                Err(e) => {
                    return Err(e.into());
                }
            }
        }
        "spinner" => {
            let mut spinner = terminal::spinner("Spinning turbines..");
            thread::sleep(time::Duration::from_secs(1));
            spinner.message("Still spinning..");
            thread::sleep(time::Duration::from_secs(1));
            spinner.message("Almost done..");
            thread::sleep(time::Duration::from_secs(1));
            spinner.message("Done.");

            spinner.finish();
        }
        "spinner-drop" => {
            let _spinner = terminal::spinner("Spinning turbines..");
            thread::sleep(time::Duration::from_secs(3));
        }
        "spinner-error" => {
            let spinner = terminal::spinner("Spinning turbines..");
            thread::sleep(time::Duration::from_secs(3));
            spinner.error("broken turbine");
        }
        "prompt" => {
            let fruit = terminal::io::select(
                "Enter your favorite fruit:",
                &["apple", "pear", "banana", "strawberry"],
                "Choose wisely!",
            )?;
            terminal::success!("You have chosen '{fruit}'");
        }
        _ => {}
    }

    Ok(())
}
