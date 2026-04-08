use radicle::Profile;
use radicle_term as term;

use super::args::DbOperation;

pub fn db(profile: &Profile, op: DbOperation) -> anyhow::Result<()> {
    match op {
        DbOperation::Exec { query } => {
            let db = profile.database_mut()?;
            db.execute(query)?;

            let changed = db.change_count();
            if changed > 0 {
                term::success!("{changed} row(s) affected.");
            } else {
                term::print(term::format::italic("No rows affected."));
            }
        }
    }
    Ok(())
}
