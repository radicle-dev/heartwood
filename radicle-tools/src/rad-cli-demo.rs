use std::{thread, time};

use radicle_cli::terminal;

fn main() -> anyhow::Result<()> {
    let demo = terminal::io::select(
        "Choose something to try out:",
        &[
            "confirm",
            "spinner",
            "spinner-drop",
            "spinner-error",
            "editor",
            "prompt",
        ],
        &"spinner",
    )?;

    match demo {
        Some(&"confirm") => {
            if terminal::confirm("Would you like to proceed?") {
                terminal::success!("You said 'yes'");
            }
        }
        Some(&"editor") => {
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
        Some(&"spinner") => {
            let mut spinner = terminal::spinner("Spinning turbines..");
            thread::sleep(time::Duration::from_secs(1));
            spinner.message("Still spinning..");
            thread::sleep(time::Duration::from_secs(1));
            spinner.message("Almost done..");
            thread::sleep(time::Duration::from_secs(1));
            spinner.message("Done.");

            spinner.finish();
        }
        Some(&"spinner-drop") => {
            let _spinner = terminal::spinner("Spinning turbines..");
            thread::sleep(time::Duration::from_secs(3));
        }
        Some(&"spinner-error") => {
            let spinner = terminal::spinner("Spinning turbines..");
            thread::sleep(time::Duration::from_secs(3));
            spinner.error("broken turbine");
        }
        Some(&"prompt") => {
            let fruit = terminal::io::select(
                "Enter your favorite fruit:",
                &["apple", "pear", "banana", "strawberry"],
                &"apple",
            )?;

            if let Some(fruit) = fruit {
                terminal::success!("You have chosen '{fruit}'");
            } else {
                terminal::info!("Ok, bye.");
            }
        }
        _ => {}
    }

    Ok(())
}
