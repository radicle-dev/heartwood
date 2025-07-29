use systemd_journal_logger::{connected_to_journal, current_exe_identifier, JournalLog};

/// If the current process is directly connected to the systemd journal,
/// return a logger that will write to it.
pub fn logger<K, V, I>(
    default_identifier: String,
    extra_fields: I,
) -> std::io::Result<Option<Box<dyn log::Log>>>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<[u8]>,
{
    if !connected_to_journal() {
        return Ok(None);
    }

    Ok(Some(Box::new(
        JournalLog::new()?
            .with_syslog_identifier(current_exe_identifier().unwrap_or(default_identifier))
            .with_extra_fields(extra_fields),
    )))
}
