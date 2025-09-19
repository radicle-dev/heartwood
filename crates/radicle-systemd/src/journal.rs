use systemd_journal_logger::{connected_to_journal, current_exe_identifier, JournalLog};

/// If the current process is directly connected to the systemd journal,
/// return a logger that will write to it.
pub fn logger<K, V, I>(
    default_identifier: String,
    extra_fields: I,
) -> std::io::Result<Box<dyn log::Log>>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<[u8]>,
{
    Ok(Box::new(
        JournalLog::new()?
            .with_syslog_identifier(current_exe_identifier().unwrap_or(default_identifier))
            .with_extra_fields(extra_fields),
    ))
}

pub fn connected() -> bool {
    connected_to_journal()
}
