//! Repositories written object by object, so a test of read_git needs no git installed.

/// A repository at `root` whose `main` holds one commit of `files` with `message`, made at
/// 2023-11-14 22:13:20 UTC. A path with a `/` is placed in a directory.
pub fn commit_files(root: &std::path::Path, files: &[(&str, &str)], message: &str) {
    use gix_object::Write as _;

    fn tree(store: &gix_odb::loose::Store, files: &[(&str, &str)]) -> gix_hash::ObjectId {
        let mut directories: std::collections::BTreeMap<&str, Vec<(&str, &str)>> =
            Default::default();
        let mut entries = Vec::new();
        for &(path, text) in files {
            match path.split_once('/') {
                Some((directory, rest)) => {
                    directories.entry(directory).or_default().push((rest, text))
                }
                None => {
                    let id = store
                        .write_buf(gix_object::Kind::Blob, text.as_bytes())
                        .expect("blob");
                    entries.push((path.as_bytes().to_vec(), format!("100644 {path}"), id));
                }
            }
        }
        for (directory, inside) in directories {
            let id = tree(store, &inside);
            entries.push((
                format!("{directory}/").into_bytes(),
                format!("40000 {directory}"),
                id,
            ));
        }
        // git orders a tree's entries by name, with a directory's name read as ending in `/`.
        entries.sort();
        let mut bytes = Vec::new();
        for (_, head, id) in entries {
            bytes.extend_from_slice(head.as_bytes());
            bytes.push(0);
            bytes.extend_from_slice(id.as_bytes());
        }
        store
            .write_buf(gix_object::Kind::Tree, &bytes)
            .expect("tree")
    }

    let git = root.join(".git");
    for directory in ["objects", "refs/heads"] {
        std::fs::create_dir_all(git.join(directory)).expect("repository");
    }
    std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").expect("HEAD");
    std::fs::write(
        git.join("config"),
        "[core]\n\trepositoryformatversion = 0\n",
    )
    .expect("config");
    let store = gix_odb::loose::Store::at(git.join("objects"), gix_hash::Kind::Sha1);
    let tree = tree(&store, files);
    let commit = store
        .write_buf(
            gix_object::Kind::Commit,
            format!(
                "tree {}\nauthor A U Thor <author@example.com> 1700000000 +0000\ncommitter C O \
                 Mitter <committer@example.com> 1700000000 +0000\n\n{message}\n",
                tree.to_hex()
            )
            .as_bytes(),
        )
        .expect("commit");
    std::fs::write(
        git.join("refs/heads/main"),
        format!("{}\n", commit.to_hex()),
    )
    .expect("main");
}
