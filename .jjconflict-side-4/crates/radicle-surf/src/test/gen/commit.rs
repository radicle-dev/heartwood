use std::convert::Infallible;

use proptest::strategy::Strategy;
use radicle_git_metadata::commit::CommitData;

mod author;
mod headers;
mod trailers;

pub use author::author;
pub use headers::headers;
pub use trailers::trailers;

use super::alphanumeric;

pub fn commit() -> impl Strategy<Value = CommitData<TreeData, Infallible>> {
    (
        TreeData::r#gen(),
        author(),
        author(),
        headers(),
        alphanumeric(),
        trailers(3),
    )
        .prop_map(|(tree, author, committer, headers, message, trailers)| {
            CommitData::new(tree, vec![], author, committer, headers, message, trailers)
        })
}

pub fn write_commits(
    repo: &git2::Repository,
    linear: Vec<CommitData<TreeData, Infallible>>,
) -> Result<Vec<git2::Oid>, git2::Error> {
    let mut parent = None;
    let mut commits = Vec::new();
    for commit in linear {
        let commit = commit.map_tree(|tree| tree.write(repo))?;
        let commit = match parent {
            Some(parent) => commit
                .map_parents::<git2::Oid, Infallible, _>(|_| Ok(parent))
                .unwrap(),
            None => commit
                .map_parents::<git2::Oid, Infallible, _>(|_| unreachable!("no parents"))
                .unwrap(),
        };
        let tree = repo.find_tree(*commit.tree())?;
        let oid = repo.commit(
            None,
            &git2::Signature::try_from(commit.author()).unwrap(),
            &git2::Signature::try_from(commit.committer()).unwrap(),
            commit.message(),
            &tree,
            &[],
        )?;
        commits.push(oid);
        parent = Some(oid);
    }
    Ok(commits)
}

#[derive(Clone, Debug)]
pub enum TreeData {
    Blob { name: String, data: String },
    Tree { name: String, inner: Vec<TreeData> },
}

impl TreeData {
    fn r#gen() -> impl Strategy<Value = Self> {
        let leaf =
            (alphanumeric(), alphanumeric()).prop_map(|(name, data)| Self::Blob { name, data });
        leaf.prop_recursive(8, 16, 5, |inner| {
            (proptest::collection::vec(inner, 1..5), alphanumeric())
                .prop_map(|(inner, name)| Self::Tree { name, inner })
        })
    }

    fn write(&self, repo: &git2::Repository) -> Result<git2::Oid, git2::Error> {
        let mut builder = repo.treebuilder(None)?;
        self.write_(repo, &mut builder)?;
        builder.write()
    }

    fn write_(
        &self,
        repo: &git2::Repository,
        builder: &mut git2::TreeBuilder,
    ) -> Result<git2::Oid, git2::Error> {
        match self {
            Self::Blob { name, data } => {
                let oid = repo.blob(data.as_bytes())?;
                builder.insert(name, oid, git2::FileMode::Blob.into())?;
            }
            Self::Tree { name, inner } => {
                for data in inner {
                    let oid = data.write_(repo, builder)?;
                    builder.insert(name, oid, git2::FileMode::Tree.into())?;
                }
            }
        }
        builder.write()
    }
}
