use std::str::FromStr;

use super::*;
use radicle::cob::patch;

use anyhow::anyhow;

pub fn parse_patch_id(val: OsString) -> Result<patch::PatchId, anyhow::Error> {
    let val = val
        .to_str()
        .ok_or_else(|| anyhow!("patch id specified is not UTF-8"))?;
    let patch_id =
        patch::PatchId::from_str(val).map_err(|_| anyhow!("invalid patch id '{}'", val))?;
    Ok(patch_id)
}
