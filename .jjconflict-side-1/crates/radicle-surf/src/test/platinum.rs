use std::{io::Read as _, mem, path::Path, str, sync::LazyLock};

use flate2::read::GzDecoder;
use git2::{Oid, Repository, Signature, Time, build::TreeUpdateBuilder};
use hex_literal::hex;
use tempfile::TempDir;

static LOCK: LazyLock<tempfile::TempDir> = LazyLock::new(init);

pub(crate) fn get() -> &'static Path {
    LOCK.path()
}

macro_rules! oid {
    ($bytes:expr) => {
        radicle_oid::Oid::Sha1(hex!($bytes))
    };
}

#[rustfmt::skip]
mod author {
    use super::*;

    pub(super) struct Author {
        name: &'static str,
        email: &'static str,
    }

    impl Author {
        const fn new(name: &'static str, email: &'static str) -> Self {
            Self { name, email }
        }

        pub(super) fn signature(&self, time: &Time) -> git2::Signature<'static> {
            Signature::new(self.name, self.email, time).unwrap()
        }
    }

    pub(super) const ALEXANDER: Author = Author::new("Alexander Simmerl",     "a.simmerl@gmail.com"     );
    pub(super) const FINTAN_1:  Author = Author::new("FintanH",         "fintan.halpenny@gmail.com"     );
    pub(super) const FINTAN_2:  Author = Author::new("Fintan Halpenny", "fintan.halpenny@gmail.com"     );
    pub(super) const GITHUB:    Author = Author::new("GitHub",                  "noreply@github.com"    );
               const HAN:       Author = Author::new("Han Xu",               "keepsimple@gmail.com"     );
    pub(super) const RUDOLFS:   Author = Author::new("Rūdolfs Ošiņš",           "rudolfs@osins.org"     );
    pub(super) const SEBASTIAN: Author = Author::new("Sebastian Martinez",           "me@sebastinez.dev");
    pub(super) const THOMAS:    Author = Author::new("Thomas Scholtes",          "thomas@monadic.xyz"   );

    pub(super) fn rudolfs(seconds: i64) -> git2::Signature<'static> {
        RUDOLFS.signature(&Time::new(seconds, 60))
    }

    pub(super) fn han(seconds: i64) -> git2::Signature<'static> {
        HAN.signature(&Time::new(seconds, -480))
    }
}

use author::*;

/// Git blobs, represented by their contents and expected object ID, i.e.,
/// expected SHA-1 digest of the contents.
///
/// [`Self::write`] asserts that the actual object ID computed matches
/// [`Blob::oid`], which helps to catch errors in construction/loading of
/// contents early.
struct Blob<'a> {
    oid: radicle_oid::Oid,
    contents: &'a [u8],
}

impl<'a> Blob<'a> {
    const EMPTY: Blob<'static> = Blob {
        oid: oid!("e69de29bb2d1d6434b8b29ae775ad8c2e48c5391"),
        contents: &[],
    };

    /// Writes this blob to the object database of `repo`, and asserts that
    /// the returned object ID matches [`Self::oid`].
    ///
    /// To obtain a handle to the blob in the object database of `repo`,
    /// use [`Repository::find_blob`] after [`Self::write`] has returned.
    fn write(&self, repo: &Repository) -> Oid {
        let oid = repo.blob(self.contents).unwrap();
        assert_eq!(radicle_oid::Oid::from(oid), self.oid);
        oid
    }

    fn to_vec(&self) -> Vec<u8> {
        self.contents.to_vec()
    }
}

const README_1: Blob = Blob {
    oid: oid!("7f48df0118b1674f4ab0ed1717c1368091a5dddc"),
    contents: b"This repository is a data source for the Upstream front-end tests.\n",
};

const README_2: Blob = Blob {
    oid: oid!("5e07534cd74a6a9b2ccd2729b181c4ef26173a5e"),
    contents: b"This repository is a data source for the Upstream front-end tests and the\n[`radicle-surf`](https://github.com/radicle-dev/git-platinum) unit tests.\n",
};

const README_3: Blob = Blob {
    oid: oid!("b033ecf407a44922b28c942c696922a7d7daf06e"),
    contents: b"This repository is a data source for the upstream front-end tests and the\n[`radicle-surf`](https://github.com/radicle-dev/radicle-surf) unit tests.\n",
};

const LICENSE: Blob = Blob {
    oid: oid!("02f70f56ec62396ceaf38804c37e169e875ab291"),
    contents: include_bytes!("platinum/LICENSE"),
};

const EVAL: Blob = Blob {
    oid: oid!("8c7447d13b907aa994ac3a38317c1e9633bf0732"),
    contents: include_bytes!("platinum/Eval.hs"),
};

const FOLDER: Blob = Blob {
    oid: oid!("a50cdb374f3a16da6cc6056a5f2818b53efdb745"),
    contents: include_bytes!("platinum/Folder.svelte"),
};

const MEMORY: Blob = Blob {
    oid: oid!("b84992d24be67536837f5ab45a943f1b3f501878"),
    contents: include_bytes!("platinum/memory.rs"),
};

const ARROWS: Blob = Blob {
    oid: oid!("95418c04010a3cc758fb3a37f9918465f147566f"),
    contents: include_bytes!("platinum/arrows.txt"),
};

const EMOJI: Blob = Blob {
    oid: oid!("1570277532948712fea9029d100a4208f9e34241"),
    contents: include_bytes!("platinum/emoji.txt"),
};

const GARDEN: Blob = Blob {
    oid: oid!("859f93b3b6a687a961ea7dd54277ae12d93567bf"),
    contents: include_bytes!("platinum/garden.txt"),
};

pub(super) fn init() -> TempDir {
    use git2::FileMode::{Blob as Regular, BlobExecutable as Executable};

    let dir = tempfile::tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let empty_tree = repo.treebuilder(None).unwrap().write().unwrap();
    let mut tree_oid = empty_tree;
    let mut tree = TreeUpdateBuilder::new();

    let empty = Blob::EMPTY.write(&repo);
    let readme_initial = README_1.write(&repo);
    let readme_updated = README_2.write(&repo);
    let readme_diff_test = README_3.write(&repo);
    let license = LICENSE.write(&repo);
    let eval_initial = EVAL.write(&repo);
    let folder = FOLDER.write(&repo);
    let memory = MEMORY.write(&repo);
    let arrows = ARROWS.write(&repo);
    let emoji = EMOJI.write(&repo);
    let garden = GARDEN.write(&repo);

    // The second version of `Eval.hs` used in tests only differs from
    // the first version by a difference of 9 bytes inserted. Instead
    // of duplicating the file, we compute the second version from the first
    // version by splicing 9 bytes.
    let eval_updated = {
        let mut contents = EVAL.to_vec();

        const INSERT: &[u8; 9] = b", the MVP";

        /// The index at which we need to insert [`INSERT`] into [`contents`].
        ///
        /// This was computed via a naive linear search:
        /// ```no_run
        /// const NEEDLE: &[u8; 16] = b", original, eval";
        /// let index = contents
        ///     .array_windows()
        ///     .enumerate()
        ///     .find_map(|(i, window)| (window == NEEDLE).then_some(i))
        ///     .unwrap()
        ///     + NEEDLE.len();
        /// ```
        const INDEX: usize = 623;

        contents.splice(INDEX..INDEX, INSERT.to_vec());

        let oid = repo.blob(&contents).unwrap();
        assert_eq!(
            radicle_oid::Oid::from(oid),
            oid!("7d6240123a8d8ea8a8376610168a0a4bcb96afd0")
        );
        oid
    };

    let ls = gunzip(
        &repo,
        oid!("87c2d5149737b266cfe35ca6fa8d2362048613ae"),
        include_bytes!("platinum/ls.gz"),
    );
    let test = gunzip(
        &repo,
        oid!("0cced79cda4babfd382621200cb6fdc6470e3c47"),
        include_bytes!("platinum/test.gz"),
    );
    let cat = gunzip(
        &repo,
        oid!("0bbb14b0f0d3b6e658879748a720ec601108d26d"),
        include_bytes!("platinum/cat.gz"),
    );

    let initial = {
        tree.upsert("README.md", readme_initial, Regular);

        let sign = rudolfs(1_575_282_266);

        commit(
            &repo,
            write_tree(&repo, &mut tree, &mut tree_oid),
            &[],
            sign.clone(),
            sign,
            "Initial commit FTW!\n",
            Some(include_str!("platinum/signatures/initial.sig")),
            oid!("d3464e33d75c75c99bfb90fa2e9d16efc0b7d0e3"),
        )
    };

    for name in [
        "refs/tags/v0.1.0",
        "refs/namespaces/golden/refs/tags/v0.1.0",
        "refs/namespaces/golden/refs/remotes/kickflip/tags/v0.1.0",
    ] {
        repo.reference(name, initial.into(), false, "").unwrap();
    }

    let nested = {
        tree.upsert(
            "this/is/a/really/deeply/nested/directory/tree/.gitkeep",
            empty,
            Regular,
        );

        let sign = rudolfs(1_575_282_370);

        commit(
            &repo,
            write_tree(&repo, &mut tree, &mut tree_oid),
            &[initial],
            sign.clone(),
            sign,
            "Add fixture for deeply nested directories\n",
            Some(include_str!("platinum/signatures/nested.sig")),
            oid!("2429f097664f9af0c5b7b389ab998b2199ffa977"),
        )
    };

    for name in [
        "refs/tags/v0.2.0",
        "refs/namespaces/golden/refs/tags/v0.2.0",
    ] {
        repo.reference(name, nested.into(), false, "").unwrap();
    }

    let sources = {
        tree.upsert("examples/Eval.hs", eval_initial, Regular);
        tree.upsert("examples/Folder.svelte", folder, Regular);
        tree.upsert("examples/memory.rs", memory, Regular);

        let sign = rudolfs(1_575_282_874);

        commit(
            &repo,
            write_tree(&repo, &mut tree, &mut tree_oid),
            &[nested],
            sign.clone(),
            sign,
            "Add some source code example files\n",
            Some(include_str!("platinum/signatures/sources.sig")),
            oid!("f3a089488f4cfd1a240a9c01b3fcc4c34a4e97b2"),
        )
    };
    let binaries = {
        tree.upsert("bin/cat", cat, Executable);
        tree.upsert("bin/ls", ls, Executable);
        tree.upsert("bin/test", test, Executable);

        let sign = rudolfs(1_575_282_964);

        commit(
            &repo,
            write_tree(&repo, &mut tree, &mut tree_oid),
            &[sources],
            sign.clone(),
            sign,
            "Add some binary files\n",
            Some(include_str!("platinum/signatures/binaries.sig")),
            oid!("19bec071db6474af89c866a1bd0e4b1ff76e2b97"),
        )
    };

    repo.reference("refs/tags/v0.3.0", binaries.into(), false, "")
        .unwrap();

    let moved = {
        tree.remove("examples/Eval.hs");
        tree.remove("examples/Folder.svelte");
        tree.remove("examples/memory.rs");
        tree.upsert("src/Eval.hs", eval_initial, Regular);
        tree.upsert("src/Folder.svelte", folder, Regular);
        tree.upsert("src/memory.rs", memory, Regular);

        let sign = rudolfs(1_575_283_266);

        commit(
            &repo,
            write_tree(&repo, &mut tree, &mut tree_oid),
            &[binaries],
            sign.clone(),
            sign,
            "Move examples to \"src\"\n",
            Some(include_str!("platinum/signatures/moved.sig")),
            oid!("e24124b7538658220b5aaf3b6ef53758f0a106dc"),
        )
    };

    let texts = {
        tree.upsert("text/arrows.txt", arrows, Regular);
        tree.upsert("text/emoji.txt", emoji, Regular);
        tree.upsert("text/garden.txt", garden, Regular);

        let sign = rudolfs(1_575_283_425);

        commit(
            &repo,
            write_tree(&repo, &mut tree, &mut tree_oid),
            &[moved],
            sign.clone(),
            sign,
            "Add text files\n",
            Some(include_str!("platinum/signatures/texts.sig")),
            oid!("1e0206da8571ca71c51c91154e2fee376e09b4e7"),
        )
    };

    let common = {
        tree.upsert(".i-am-well-hidden", empty, Regular);
        tree.upsert(".i-too-am-hidden", empty, Regular);

        let sign = rudolfs(1_575_283_503);

        commit(
            &repo,
            write_tree(&repo, &mut tree, &mut tree_oid),
            &[texts],
            sign.clone(),
            sign,
            "Add dotfiles\n",
            Some(include_str!("platinum/signatures/common.sig")),
            oid!("1820cb07c1a890016ca5578aa652fd4d4c38967e"),
        )
    };

    let common_tree = tree_oid;
    let dev = {
        tree.upsert("here-we-are-on-a-dev-branch.lol", empty, Regular);
        let sign = rudolfs(1_575_283_616);

        commit(
            &repo,
            write_tree(&repo, &mut tree, &mut tree_oid),
            &[common],
            sign.clone(),
            sign,
            "Commit on the dev branch\n",
            Some(include_str!("platinum/signatures/dev.sig")),
            oid!("27acd68c7504755aa11023300890bb85bbd69d45"),
        )
    };

    for name in [
        "refs/heads/dev",
        "refs/namespaces/golden/refs/heads/banana",
        "refs/namespaces/golden/refs/namespaces/silver/refs/heads/master",
        "refs/namespaces/golden/refs/remotes/kickflip/heads/fakie/bigspin",
        "refs/namespaces/golden/refs/remotes/kickflip/heads/heelflip",
        "refs/namespaces/me/refs/remotes/fein/heads/feature/#1194",
        "refs/remotes/origin/dev",
    ] {
        repo.reference(name, dev.into(), false, "").unwrap();
    }

    tree_oid = common_tree;

    let deletion_added = {
        tree.upsert("test-file-deletion.txt", empty, Regular);

        let sign = rudolfs(1_575_468_360);

        commit(
            &repo,
            write_tree(&repo, &mut tree, &mut tree_oid),
            &[common],
            sign.clone(),
            sign,
            "Add file which will be deleted later\n",
            Some(include_str!("platinum/signatures/deletion-added.sig")),
            oid!("91b69e00cd8e5a07e20942e9e4457d83ce7a3ff1"),
        )
    };

    repo.reference("refs/tags/v0.4.0", deletion_added.into(), false, "")
        .unwrap();

    let deletion_removed = {
        tree.remove("test-file-deletion.txt");

        let sign = rudolfs(1_575_468_397);

        commit(
            &repo,
            write_tree(&repo, &mut tree, &mut tree_oid),
            &[deletion_added],
            sign.clone(),
            sign,
            "Delete unneeded file\n",
            Some(include_str!("platinum/signatures/deletion-removed.sig")),
            oid!("80ded66281a4de2889cc07293a8f10947c6d57fe"),
        )
    };

    repo.reference("refs/tags/v0.5.0", deletion_removed.into(), false, "")
        .unwrap();

    let long_message = {
        let sign = |author: Author| author.signature(&Time::new(1_576_170_713, 60));

        commit(
            &repo,
            write_tree(&repo, &mut tree, &mut tree_oid),
            &[deletion_removed],
            sign(ALEXANDER),
            sign(GITHUB),
            "Add a long commit message to commit message body (#1)\n\nIn order to test the correct delivery of the message part of the commit\r\nwe add this commit which has both by expanding beyond the summary.",
            Some(include_str!("platinum/signatures/long-message.sig")),
            oid!("d6880352fc7fda8f521ae9b7357668b17bb5bad5"),
        )
    };

    {
        let target = repo.find_object(long_message.into(), None).unwrap();
        let tagger = THOMAS.signature(&Time::new(1_620_740_737, 120));
        let tag = repo
            .tag(
                "v0.6.0",
                &target,
                &tagger,
                "An annotated tag message for v0.6.0\n",
                false,
            )
            .unwrap();
        assert_eq!(
            radicle_oid::Oid::from(tag),
            oid!("4d1f4af2703074d37cb877f4fdbe36322c8e541d")
        );
    }

    let long_message_tree = tree_oid;
    let readme = {
        tree.upsert("README.md", readme_updated, Regular);

        let sign = |seconds| FINTAN_1.signature(&Time::new(seconds, 0));

        commit(
            &repo,
            write_tree(&repo, &mut tree, &mut tree_oid),
            &[long_message],
            sign(1_584_362_521),
            sign(1_584_362_684),
            "Updated README with radicle-surf link\n",
            None,
            oid!("80bacafba303bf0cdf6142921f430ff265f25095"),
        )
    };
    tree_oid = long_message_tree;

    let docs = {
        tree.upsert("src/Eval.hs", eval_updated, Regular);

        let sign = |author: Author| author.signature(&Time::new(1_578_309_972, 0));

        commit(
            &repo,
            write_tree(&repo, &mut tree, &mut tree_oid),
            &[long_message],
            sign(FINTAN_2),
            sign(GITHUB),
            "Extend the docs (#2)\n\nI want to have files under src that have separate commits.\r\nThat way src's latest commit isn't the same as all its files, instead it's the file that was touched last.",
            Some(include_str!("platinum/signatures/docs.sig")),
            oid!("3873745c8f6ffb45c990eb23b491d4b4b6182f95"),
        )
    };

    let folder_removed = {
        tree.remove("src/Folder.svelte");

        let sign = |author: Author| author.signature(&Time::new(1_582_198_877, 60));

        commit(
            &repo,
            write_tree(&repo, &mut tree, &mut tree_oid),
            &[docs],
            sign(RUDOLFS),
            sign(GITHUB),
            "Remove src/Folder.svelte (#3)\n\nIt was a bad idea to have an actual source file which is used by\r\nradicle-upstream in the fixtures repository. It gets in the way of\r\nlinting and editors pick it up as a regular source file by accident.",
            Some(include_str!("platinum/signatures/folder-removed.sig")),
            oid!("a57846bbc8ced6587bf8329fc4bce970eb7b757e"),
        )
    };

    let merged = {
        tree.upsert("README.md", readme_updated, Regular);

        let sign = |author: Author| author.signature(&Time::new(1_584_367_899, 60));

        commit(
            &repo,
            write_tree(&repo, &mut tree, &mut tree_oid),
            &[folder_removed, readme],
            sign(ALEXANDER),
            sign(GITHUB),
            "Merge pull request #4 from FintanH/fintan/update-readme-no-sig\n\nUpdated README",
            Some(include_str!("platinum/signatures/merged.sig")),
            oid!("223aaf87d6ea62eef0014857640fd7c8dd0f80b5"),
        )
    };

    let master = {
        for path in [
            "special/-dash-",
            "special/...",
            "special/:colon:",
            "special/;semicolon;",
            "special/@at@",
            "special/_underscore_",
            "special/c++",
            "special/faux\\path",
            "special/i need some space",
            "special/qs?param1=value?param2=value2#hash",
            "special/~tilde~",
            "special/👹👹👹",
        ] {
            tree.upsert(path, empty, Regular);
        }

        let sign = |author: Author| author.signature(&Time::new(1_602_778_504, 120));

        commit(
            &repo,
            write_tree(&repo, &mut tree, &mut tree_oid),
            &[merged],
            sign(RUDOLFS),
            sign(GITHUB),
            "Add files with special characters in their filenames (#5)\n\n",
            Some(include_str!("platinum/signatures/master.sig")),
            oid!("a0dd9122d33dff2a35f564d564db127152c88e02"),
        )
    };

    for name in [
        "refs/heads/master",
        "refs/namespaces/golden/refs/heads/master",
        "refs/namespaces/me/refs/heads/feature/#1194",
        "refs/remotes/banana/orange/pineapple",
        "refs/remotes/banana/pineapple",
        "refs/remotes/origin/master",
    ] {
        repo.reference(name, master.into(), false, "").unwrap();
    }

    let master_tree = tree_oid;
    tree_oid = empty_tree;

    let empty_branch = {
        let sign = han(1_674_618_111);
        let removed_all = commit(
            &repo,
            write_tree(&repo, &mut tree, &mut tree_oid),
            &[master],
            sign.clone(),
            sign,
            "remove all files\n",
            None,
            oid!("13a866a47591625c3fb7895e454369cd5874badc"),
        );

        let sign = han(1_674_618_130);
        commit(
            &repo,
            write_tree(&repo, &mut tree, &mut tree_oid),
            &[removed_all],
            sign.clone(),
            sign,
            "an empty commit.\n",
            None,
            oid!("e972683fe8136bf8a5cb2378cf50303554008049"),
        )
    };

    for name in [
        "refs/heads/empty-branch",
        "refs/remotes/origin/empty-branch",
    ] {
        repo.reference(name, empty_branch.into(), false, "")
            .unwrap();
    }

    let diff_test = {
        tree_oid = master_tree;
        tree.upsert("LICENSE", license, Regular);
        tree.upsert("README.md", readme_diff_test, Regular);
        tree.remove("text/emoji.txt");
        tree.upsert("emoji.txt", emoji, Regular);
        tree.upsert("file_operations/copied.md", readme_updated, Regular);
        tree.remove("text/arrows.txt");

        commit(
            &repo,
            write_tree(&repo, &mut tree, &mut tree_oid),
            &[master],
            han(1_675_087_389),
            SEBASTIAN.signature(&Time::new(1_687_938_471, 120)),
            "One commit to include add, delete, modify, copy and move\n\nFor copies detection, we copy the README file which is also being\nmodified in the same changeset.\nDue to unmodified copies detection being to expensive.\n",
            Some(include_str!("platinum/signatures/diff-test.sig")),
            oid!("29b78a041bffb955b597719b27c51134a49555c1"),
        )
    };

    for name in ["refs/heads/diff-test", "refs/remotes/origin/diff-test"] {
        repo.reference(name, diff_test.into(), false, "").unwrap();
    }

    repo.reference_symbolic(
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/master",
        false,
        "create platinum fixture",
    )
    .unwrap();

    repo.set_head("refs/heads/dev").unwrap();

    dir
}

fn gunzip(repo: &Repository, expected: radicle_oid::Oid, bytes: &[u8]) -> Oid {
    let mut decoded = Vec::new();
    GzDecoder::new(bytes).read_to_end(&mut decoded).unwrap();
    let oid = repo.blob(&decoded).unwrap();
    assert_eq!(radicle_oid::Oid::from(oid), expected);
    oid
}

fn write_tree(
    repo: &Repository,
    builder: &mut TreeUpdateBuilder,
    baseline: &mut Oid,
) -> radicle_oid::Oid {
    let tree = repo.find_tree(*baseline).unwrap();
    *baseline = mem::take(builder).create_updated(repo, &tree).unwrap();
    (*baseline).into()
}

#[allow(clippy::too_many_arguments)]
fn commit(
    repo: &Repository,
    tree: radicle_oid::Oid,
    parents: &[radicle_oid::Oid],
    author: Signature<'_>,
    committer: Signature<'_>,
    message: &str,
    signature: Option<&str>,
    expected: radicle_oid::Oid,
) -> radicle_oid::Oid {
    let tree = repo.find_tree(tree.into()).unwrap();
    let parents = parents
        .iter()
        .map(|oid| repo.find_commit(oid.into()).unwrap())
        .collect::<Vec<_>>();
    let parent_refs = parents.iter().collect::<Vec<_>>();

    let oid = match signature {
        Some(signature) => {
            let buffer = repo
                .commit_create_buffer(&author, &committer, message, &tree, &parent_refs)
                .unwrap();
            let content = str::from_utf8(&buffer).unwrap();
            repo.commit_signed(content, signature, None).unwrap()
        }
        None => repo
            .commit(None, &author, &committer, message, &tree, &parent_refs)
            .unwrap(),
    };

    let oid = oid.into();
    assert_eq!(oid, expected);
    oid
}
