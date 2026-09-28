//! Persistent mapping between external markdown issue ids and Radicle object
//! ids.
//!
//! When an imported markdown id differs from the id of the freshly created
//! Radicle issue, the pair is recorded here so subsequent imports resolve to
//! the same local issue instead of creating duplicates.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context as _;

use super::write_atomic;

pub(super) const ID_MAP_FILE_NAME: &str = ".radicle-issue-import-map.json";

pub(super) fn id_map_path(issue_dir: &Path) -> PathBuf {
    issue_dir.join(ID_MAP_FILE_NAME)
}

pub(super) fn load_id_map(issue_dir: &Path) -> anyhow::Result<BTreeMap<String, String>> {
    let path = id_map_path(issue_dir);
    if !path.exists() {
        return Ok(BTreeMap::new());
    }

    let raw = fs::read_to_string(&path)
        .with_context(|| format!("failed to read issue id mapping file '{}'", path.display()))?;
    let map = serde_json::from_str::<BTreeMap<String, String>>(&raw)
        .with_context(|| format!("failed to parse issue id mapping file '{}'", path.display()))?;

    Ok(map)
}

pub(super) fn save_id_map(issue_dir: &Path, map: &BTreeMap<String, String>) -> anyhow::Result<()> {
    let mut content = serde_json::to_string_pretty(map)?;
    content.push('\n');
    write_atomic(id_map_path(issue_dir).as_path(), &content)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;

    use super::{ID_MAP_FILE_NAME, load_id_map, save_id_map};

    #[test]
    fn load_id_map_returns_empty_for_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let map = load_id_map(tmp.path()).unwrap();

        assert!(map.is_empty());
    }

    #[test]
    fn save_id_map_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let mut map = BTreeMap::new();
        map.insert(
            "external-one".to_owned(),
            "1111111111111111111111111111111111111111".to_owned(),
        );

        save_id_map(tmp.path(), &map).unwrap();
        let loaded = load_id_map(tmp.path()).unwrap();

        assert_eq!(loaded, map);
        assert!(tmp.path().join(ID_MAP_FILE_NAME).exists());
        assert!(
            fs::read_to_string(tmp.path().join(ID_MAP_FILE_NAME))
                .unwrap()
                .contains("external-one")
        );
    }
}
