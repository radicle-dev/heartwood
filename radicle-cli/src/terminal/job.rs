use radicle::cob;
use radicle::cob::job;
use radicle_term::table::TableOptions;
use radicle_term::{Table, VStack};

use crate::terminal as term;
use crate::terminal::Element;

pub fn show(job: &job::Job, id: &cob::ObjectId) -> anyhow::Result<()> {
    let mut attrs = Table::<2, term::Line>::new(TableOptions {
        spacing: 2,
        ..TableOptions::default()
    });

    attrs.push([
        term::format::tertiary("Job".to_owned()).into(),
        term::format::bold(id.to_string()).into(),
    ]);

    attrs.push([
        term::format::tertiary("Commit".to_owned()).into(),
        term::format::bold(job.commit().to_owned()).into(),
    ]);

    attrs.push([
        term::format::tertiary("State".to_owned()).into(),
        term::format::bold(job.state().to_string()).into(),
    ]);

    if let Some(run_id) = job.run_id() {
        attrs.push([
            term::format::tertiary("Run ID".to_owned()).into(),
            term::format::bold(run_id.to_string()).into(),
        ]);
    }

    if let Some(info_url) = job.info_url() {
        attrs.push([
            term::format::tertiary("Info URL".to_owned()).into(),
            term::format::bold(info_url.to_string()).into(),
        ]);
    }

    let widget = VStack::default()
        .border(Some(term::colors::FAINT))
        .child(attrs);

    widget.print();

    Ok(())
}
