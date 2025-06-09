//! Migration to update the patch `reviews` JSON representation.
use crate::cob::cache::*;
use serde_json as json;

/// Run migration.
pub fn run(
    db: &sql::Connection,
    migration: &Progress,
    callback: &mut dyn MigrateCallback,
) -> Result<usize, Error> {
    // Select patches with reviews.
    let rows = db
        .prepare("SELECT id, patch FROM patches WHERE json_extract(patch, '$.reviews') != '{}'")?
        .into_iter()
        .collect::<Vec<_>>();
    // Query to update a patch to the new schema.
    let mut update = db.prepare(
        "UPDATE patches
         SET patch = ?1
         WHERE id = ?2",
    )?;
    let mut progress = Progress::new(rows.len());
    callback.progress(MigrateProgress {
        migration,
        rows: &progress,
    });

    for row in rows {
        let row = row?;
        let id = row.read::<&str, _>("id");
        let mut patch = json::from_str::<json::Value>(row.read::<&str, _>("patch"))
            .map_err(Error::MalformedJson)?;
        let patch = patch.as_object_mut().ok_or(Error::MalformedJsonSchema)?;
        let revisions = patch["revisions"]
            .as_object_mut()
            .ok_or(Error::MalformedJsonSchema)?;
        let mut transformed = false;

        for (_, r) in revisions.iter_mut() {
            let Some(revision) = r.as_object_mut() else {
                // Redacted revision (`null`).
                continue;
            };
            let reviews = revision
                .get_mut("reviews")
                .ok_or(Error::MalformedJsonSchema)?
                .as_object_mut()
                .ok_or(Error::MalformedJsonSchema)?;

            for (_, review) in reviews.iter_mut() {
                if let Some(list) = review.as_array_mut() {
                    if let Some(last) = list.pop() {
                        *review = last;
                        transformed = true;
                    }
                }
            }
        }

        if transformed {
            let obj = json::to_string(&patch).map_err(Error::MalformedJson)?;

            update.reset()?;
            update.bind((1, obj.as_str()))?;
            update.bind((2, id))?;
            update.next()?;
            progress.inc();

            callback.progress(MigrateProgress {
                migration,
                rows: &progress,
            });
        }
    }
    Ok(progress.current())
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use crate::cob::cache::*;

    // Before the migration.
    const PATCH_V1: &str = include_str!("samples/patch.v1.json");
    // After the migration.
    const PATCH_V2: &str = include_str!("samples/patch.v2.json");

    #[test]
    fn test_migration_2() {
        let mut db = StoreWriter::memory().unwrap();
        db.migrate_to(1, migrate::ignore).unwrap();
        db.raw_query(|conn| {
            let mut stmt = conn.prepare(
                "INSERT INTO patches (id, repo, patch)
                 VALUES (?1, ?2, ?3)",
            )?;
            stmt.bind((1, "016a91d2029ee71b9aee8d927664caf1b7885346"))?;
            stmt.bind((2, "rad:z4V1sjrXqjvFdnCUbxPFqd5p4DtH5"))?;
            stmt.bind((3, PATCH_V1))?;
            stmt.next()?;

            Ok::<_, sql::Error>(())
        })
        .unwrap();

        assert_eq!(db.migrate_to(2, migrate::ignore).unwrap(), 2);

        let row = db
            .raw_query(|conn| {
                Ok::<_, sql::Error>(
                    conn.prepare("SELECT patch FROM patches LIMIT 1")?
                        .into_iter()
                        .next()
                        .unwrap()
                        .unwrap(),
                )
            })
            .unwrap();

        let patch = row.read::<&str, _>("patch");
        let actual: serde_json::Value = serde_json::from_str(patch).unwrap();
        let expected: serde_json::Value = serde_json::from_str(PATCH_V2).unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_patch_json_deserialization() {
        serde_json::from_str::<crate::cob::patch::Patch>(PATCH_V1).unwrap_err();
        serde_json::from_str::<crate::cob::patch::Patch>(PATCH_V2).unwrap();
    }
}
