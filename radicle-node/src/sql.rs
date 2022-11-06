use sqlite as sql;

/// Run an SQL query inside a transaction.
/// Commits the transaction on success, and rolls back on error.
pub fn transaction<T>(
    db: &sql::Connection,
    query: impl FnOnce(&sql::Connection) -> Result<T, sql::Error>,
) -> Result<T, sql::Error> {
    db.execute("BEGIN")?;

    match query(db) {
        Ok(result) => {
            db.execute("COMMIT")?;
            Ok(result)
        }
        Err(err) => {
            db.execute("ROLLBACK")?;
            Err(err)
        }
    }
}
