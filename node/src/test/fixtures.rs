use std::path::Path;
use std::str::FromStr;

use once_cell::sync::Lazy;

use crate::identity::{ProjId, UserId};
use crate::storage::git::Storage;
use crate::storage::{WriteRepository, WriteStorage};

pub static USER_IDS: Lazy<[UserId; 16]> = Lazy::new(|| {
    [
        UserId::from_str("zCBH2FXDERonR1rDopkZTKAZAFKpXkiFc46XHYz1Qcyb4").unwrap(),
        UserId::from_str("z68bx7oq3JX3d5RVQSZzh7S9qP6a7AFNQnRas8EiELeGm").unwrap(),
        UserId::from_str("z8QVc8haUs3rc23ZfQYcFdntDCE5TGVWGhB1Piit2FyLW").unwrap(),
        UserId::from_str("zAmTnRTXhk49nShSWDFvi93SMFgTRGKAAJyHpXJ4rAFb8").unwrap(),
        UserId::from_str("z4RqaY63zcPZY2UudyUxfUrWu2FpTLMrCCfskah6YWxww").unwrap(),
        UserId::from_str("zHvS7fNSn3b8kE9Vy83wuitRTqkyqvRC7G7q8B1kbxPfG").unwrap(),
        UserId::from_str("z9Q8YnVrkpTCEx4ffFgXhEMNSbwX7unMJYLLTNF78Vjd9").unwrap(),
        UserId::from_str("zBTfXzjQhNgob6f5D3rhpJjTpFzjbLzU85QoKdS3CgT6v").unwrap(),
        UserId::from_str("z8qQNwfQZqYPQp9xyDssZyh7QctGWAjvq7A2T8vE4oWLr").unwrap(),
        UserId::from_str("z5YzpLeMn6ozf95bELziodGNpyTs5jQ7ssfofdv4rhB92").unwrap(),
        UserId::from_str("z8YUtfXfp5bthrongT11C9fYcSsT6QKY1SgfENxB5KXvj").unwrap(),
        UserId::from_str("zBSUVcBSUWPtYWoPPxZg9QPDDTFVQe2dZaXLaCvFvd9Di").unwrap(),
        UserId::from_str("z6ba5eWTvR22JL4ej4qErnGcxJTZF3YCHEH9FMriQbsmj").unwrap(),
        UserId::from_str("z6MJB1N1WfwWzt69k39eRmHGZ7CCefo4h68zX1gBEWKyh").unwrap(),
        UserId::from_str("zD2UwYEK4FGcrX7HqAPnT5i42uYG6ZgeeSo3C52a21ktJ").unwrap(),
        UserId::from_str("z5NVinWZWpNz7EbU26mdZ2nQV3inK3ZHw3YuW8Dd6puyw").unwrap(),
    ]
});

pub static PROJ_IDS: Lazy<[ProjId; 16]> = Lazy::new(|| {
    [
        ProjId::from_str("z3VDhnNMUwoaQHxNm5iNMEYuwY6RRi1e6WZ74oJKAWJzS").unwrap(),
        ProjId::from_str("zZXpj5rW3GsGnBXhszTYN5hru45AoKdqc7rbD7KK23y2").unwrap(),
        ProjId::from_str("z35ytZY1YSnk2M7Riz9KbEVfdrWAbrLdLKP5bpgPZR1uM").unwrap(),
        ProjId::from_str("zJDZF6mV6g1owvYnqbc8rGzYxbigsP7SAMkFZZXnwwxyc").unwrap(),
        ProjId::from_str("z7vwbcQRR8nu3GaHSyxQd2AvQuVsiKEFtV4EoBZGGSECn").unwrap(),
        ProjId::from_str("zDNvLCdwYAsRzH2UQzb1D5CjV5xf2rDsARsFXUymqswJF").unwrap(),
        ProjId::from_str("zAo9SpyTcwYxSe3ReaZj42T2zAFik3gY2A2eJhchLrArA").unwrap(),
        ProjId::from_str("zFXbHCdxjJpYJ1rYW8T7qUoZnkMcPwxR3xARZZpSqdGgg").unwrap(),
        ProjId::from_str("zDT5dWPNUudw9T8gD2vBZ6RZ6tXjzgnvoj1UdMnCGHeFr").unwrap(),
        ProjId::from_str("z7aD9ReLj8RbchMJuVgy928oT764iWq7p4wKCwWqroH7V").unwrap(),
        ProjId::from_str("z3PRwULg4pDXpv5GJe2z563hM7YuJFQdWPbouFkpEfUoS").unwrap(),
        ProjId::from_str("zBTQxg8xG8gGqUFtJwWotft3QuFuPPhA1aEBFLhqHuX4c").unwrap(),
        ProjId::from_str("z8exxN2CzTJDcFC5n8CPzskrpUYdu7rzupwCzjTUgmtb4").unwrap(),
        ProjId::from_str("z5Fa2PbkXKMLnvf4ZEbdHDiTmKVcqKRgxZn8xwVBZHkn1").unwrap(),
        ProjId::from_str("zEG2mTq7ExW3wBEgjwUDHqDQXZgamyv8cDmZud3b27SaJ").unwrap(),
        ProjId::from_str("z3dCpDtvBFj55HLvEW3grgqiYeanNHxW1hYS1TSK8epS4").unwrap(),
    ]
});

pub fn storage<P: AsRef<Path>>(path: P) -> Storage {
    let path = path.as_ref();
    let storage = Storage::new(path);

    for proj in PROJ_IDS.iter().take(3) {
        log::debug!("creating {}...", proj);
        let mut repo = storage.repository(proj).unwrap();

        for user in USER_IDS.iter().take(3) {
            let repo = repo.namespace(user).unwrap();
            let head_oid = initial_commit(repo).unwrap();
            let head = repo.find_commit(head_oid).unwrap();

            log::debug!("{}: creating {}...", proj, repo.namespace().unwrap());

            repo.reference("refs/rad/root", head_oid, false, "test")
                .unwrap();

            // TODO: Different commits.
            repo.branch("master", &head, false).unwrap();
            repo.branch("patch/3", &head, false).unwrap();
        }
    }
    storage
}

/// Create an initial empty commit.
fn initial_commit(repo: &git2::Repository) -> Result<git2::Oid, git2::Error> {
    let sig = git2::Signature::now("cloudhead", "cloudhead@radicle.xyz")?;
    // Now let's create an empty tree for this commit.
    let tree_id = repo.index()?.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let oid = repo.commit(None, &sig, &sig, "Initial commit", &tree, &[])?;

    Ok(oid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        let path = tempfile::tempdir().unwrap().into_path();

        storage(&path);
    }
}
