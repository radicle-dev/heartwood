use dialoguer::console::style;
use indicatif::{ProgressBar, ProgressFinish, ProgressStyle};

use crate::terminal as term;

pub struct Spinner {
    progress: ProgressBar,
    message: String,
}

impl Drop for Spinner {
    fn drop(&mut self) {
        // TODO: Set error that will be output on fail.
        if !self.progress.is_finished() {
            self.set_failed();
        }
    }
}

impl Spinner {
    pub fn finish(&self) {
        self.progress.finish_and_clear();
        term::success!("{}", &self.message);
    }

    pub fn done(self) {
        self.progress.finish_and_clear();
        term::info!("{}", &self.message);
    }

    pub fn failed(mut self) {
        self.set_failed();
    }

    pub fn error(mut self, msg: impl ToString) {
        let msg = msg.to_string();

        self.message = format!("{} ({})", self.message, msg);
        self.set_failed();
    }

    pub fn clear(self) {
        self.progress.finish_and_clear();
    }

    pub fn message(&mut self, msg: impl ToString) {
        let msg = msg.to_string();

        self.progress.set_message(msg.clone());
        self.message = msg;
    }

    pub fn set_failed(&mut self) {
        self.progress.finish_and_clear();
        term::eprintln(style("!!").red().reverse(), &self.message);
    }
}

pub fn spinner(message: impl ToString) -> Spinner {
    let message = message.to_string();
    let style = ProgressStyle::default_spinner()
        .tick_strings(&[
            &style("\\ ").yellow().to_string(),
            &style("| ").yellow().to_string(),
            &style("/ ").yellow().to_string(),
            &style("| ").yellow().to_string(),
        ])
        .template("{spinner} {msg}")
        .on_finish(ProgressFinish::AndClear);

    let progress = ProgressBar::new(!0);
    progress.set_style(style);
    progress.enable_steady_tick(99);
    progress.set_message(message.clone());

    Spinner { message, progress }
}
