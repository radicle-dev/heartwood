//! Unit tests for radicle_surf::file_system

mod directory {
    use radicle_git_ext::ref_format::refname;
    use radicle_surf::{
        fs::{self, Entry},
        Branch, Oid, Repository,
    };
    use std::path::Path;

    const GIT_PLATINUM: &str = "../data/git-platinum";

    #[test]
    fn directory_find_entry() {
        let repo = Repository::open(GIT_PLATINUM).unwrap();
        let root = repo.root_dir(Branch::local(refname!("master"))).unwrap();

        // find_entry for a file.
        let path = Path::new("src/memory.rs");
        let entry = root.find_entry(&path, &repo).unwrap();
        assert!(matches!(entry, fs::Entry::File(_)));

        // find_entry for a directory.
        let path = Path::new("this/is/a/really/deeply/nested/directory/tree");
        let entry = root.find_entry(&path, &repo).unwrap();
        assert!(matches!(entry, fs::Entry::Directory(_)));

        // find_entry for a non-leaf directory and its relative path.
        let path = Path::new("text");
        let entry = root.find_entry(&path, &repo).unwrap();
        assert!(matches!(entry, fs::Entry::Directory(_)));
        if let fs::Entry::Directory(sub_dir) = entry {
            let inner_path = Path::new("garden.txt");
            let inner_entry = sub_dir.find_entry(&inner_path, &repo).unwrap();
            assert!(matches!(inner_entry, fs::Entry::File(_)));
        }

        // find_entry for non-existing file
        let path = Path::new("this/is/a/really/missing_file");
        let result = root.find_entry(&path, &repo);
        assert!(matches!(result, Err(fs::error::Directory::PathNotFound(_))));

        // find_entry for absolute path: fail.
        let path = Path::new("/src/memory.rs");
        let result = root.find_entry(&path, &repo);
        assert!(result.is_err());

        // find entry for an empty path
        let path = Path::new("");
        let result = root.find_entry(&path, &repo);
        assert!(result.is_err());
    }

    #[test]
    fn directory_find_file_and_directory() {
        let repo = Repository::open(GIT_PLATINUM).unwrap();
        // Get the snapshot of the directory for a given commit.
        let root = repo
            .root_dir("80ded66281a4de2889cc07293a8f10947c6d57fe")
            .unwrap();

        // Assert that we can find the memory.rs file!
        assert!(root.find_file(&Path::new("src/memory.rs"), &repo).is_ok());

        let root_contents: Vec<Entry> = root.entries(&repo).unwrap().collect();
        assert_eq!(root_contents.len(), 7);
        assert!(root_contents[0].is_file());
        assert!(root_contents[1].is_file());
        assert!(root_contents[2].is_file());
        assert_eq!(root_contents[0].name(), ".i-am-well-hidden");
        assert_eq!(root_contents[1].name(), ".i-too-am-hidden");
        assert_eq!(root_contents[2].name(), "README.md");

        assert!(root_contents[3].is_directory());
        assert!(root_contents[4].is_directory());
        assert!(root_contents[5].is_directory());
        assert!(root_contents[6].is_directory());
        assert_eq!(root_contents[3].name(), "bin");
        assert_eq!(root_contents[4].name(), "src");
        assert_eq!(root_contents[5].name(), "text");
        assert_eq!(root_contents[6].name(), "this");

        let src = root.find_directory(&Path::new("src"), &repo).unwrap();
        assert_eq!(src.path(), Path::new("src").to_path_buf());
        let src_contents: Vec<Entry> = src.entries(&repo).unwrap().collect();
        assert_eq!(src_contents.len(), 3);
        assert_eq!(src_contents[0].name(), "Eval.hs");
        assert_eq!(src_contents[1].name(), "Folder.svelte");
        assert_eq!(src_contents[2].name(), "memory.rs");
    }

    #[test]
    fn directory_size() {
        let repo = Repository::open(GIT_PLATINUM).unwrap();
        let root = repo.root_dir(Branch::local(refname!("master"))).unwrap();

        /*
        git-platinum (master) $ ls -l src
        -rw-r--r-- 1 pi pi 10044 Oct 31 11:32 Eval.hs
        -rw-r--r-- 1 pi pi  6253 Oct 31 11:27 memory.rs

        10044 + 6253 = 16297
         */

        let path = Path::new("src");
        let entry = root.find_entry(&path, &repo).unwrap();
        assert!(matches!(entry, fs::Entry::Directory(_)));
        if let fs::Entry::Directory(d) = entry {
            assert_eq!(16297, d.size(&repo).unwrap());
        }
    }

    #[test]
    fn directory_last_commit() {
        let repo = Repository::open(GIT_PLATINUM).unwrap();
        let branch = Branch::local(refname!("dev"));
        let root = repo.root_dir(&branch).unwrap();
        let dir = root.find_directory(&"this/is", &repo).unwrap();
        let last_commit = repo.last_commit(&dir.path(), &branch).unwrap().unwrap();
        assert_eq!(
            last_commit.id.to_string(),
            "2429f097664f9af0c5b7b389ab998b2199ffa977"
        );
    }

    #[test]
    fn file_last_commit() {
        let repo = Repository::open(GIT_PLATINUM).unwrap();
        let branch = Branch::local(refname!("master"));
        let root = repo.root_dir(&branch).unwrap();

        // Find a file with "\" in its name.
        let f = root.find_file(&"special/faux\\path", &repo).unwrap();
        let last_commit = repo.last_commit(&f.path(), &branch).unwrap().unwrap();
        assert_eq!(
            last_commit.id.to_string(),
            "a0dd9122d33dff2a35f564d564db127152c88e02"
        );
    }

    /// Test that directories and files with glob metacharacters in their names
    /// can be browsed and have their history retrieved correctly.
    ///
    /// This is a regression test for a bug where paths containing `[` were
    /// interpreted as glob patterns by git's pathspec, causing errors.
    #[test]
    fn directory_with_bracket_in_name() {
        let repo = test_helpers::tempdir::WithTmpDir::new(|path| {
            git2::Repository::init(path).map_err(std::io::Error::other)
        })
        .unwrap();

        // Initialize the repo and create test structure:
        // src/
        //   [special-dir]/
        //     file.txt
        //   normal-file.txt
        let commit = {
            let mut tb = repo.treebuilder(None).unwrap();
            let hello = repo.blob(b"hello world").unwrap();
            tb.insert("file.txt", hello, git2::FileMode::Blob.into())
                .unwrap();
            let inner = tb.write().unwrap();
            let mut tb = repo.treebuilder(None).unwrap();
            let normal = repo.blob(b"normal content").unwrap();
            tb.insert("normal-file.txt", normal, git2::FileMode::Blob.into())
                .unwrap();
            tb.insert("[special-dir]", inner, git2::FileMode::Tree.into())
                .unwrap();
            let id = tb.write().unwrap();
            let mut tb = repo.treebuilder(None).unwrap();
            tb.insert("src", id, git2::FileMode::Tree.into()).unwrap();
            let tree = tb.write().unwrap();
            let tree = repo.find_tree(tree).unwrap();
            let sig = git2::Signature::now("Test", "test@test.com").unwrap();
            Oid::from(
                repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
                    .unwrap(),
            )
        };

        let repo = Repository::open(repo.path()).unwrap();
        let branch = Branch::local(refname!("master"));
        let root = repo.root_dir(&branch).unwrap();

        let src = root.find_directory(&"src", &repo).unwrap();
        let src_entries: Vec<Entry> = src.entries(&repo).unwrap().collect();
        assert_eq!(src_entries.len(), 2);

        let special_dir = src.find_directory(&"[special-dir]", &repo).unwrap();
        assert_eq!(special_dir.name(), "[special-dir]");

        let dir_path = special_dir.path();
        let dir_last_commit = repo.last_commit(&dir_path, &branch).unwrap();
        assert_eq!(dir_last_commit.map(|c| c.id), Some(commit));

        let file = special_dir.find_file(&"file.txt", &repo).unwrap();
        let file_path = file.path();
        let file_last_commit = repo.last_commit(&file_path, &branch).unwrap();
        assert_eq!(file_last_commit.map(|c| c.id), Some(commit));
    }
}
