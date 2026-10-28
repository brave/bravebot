//! What `make cli-repo-metadata-deb` and `make cli-repo-metadata-rpm` write from the CLI's packages,
//! and the settings the install instructions give a client for the repositories they describe.
//!
//! [RELEASE-22] is the clause. The targets are shell in a makefile, so running them against a copy
//! of the Makefile in a scratch directory is the only way a Rust test can hold them to it.
//! `apt-ftparchive` and `createrepo_c` are made here: each records its arguments and writes what
//! the real one writes the shape of, a package listing and a `Release` that lists the files beside
//! it, and `repodata/repomd.xml`. What the real tools write was not compared, and neither was
//! installing from the result on a distribution.
//!
//! Unix only, because the stand-in tools are shell scripts and the targets are run by `make`.
//! Skipped where `make` is not on `PATH`, and says so.
//!
//! [RELEASE-22]: ../../../docs/specs/releases.md

#![cfg(unix)]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const VERSION: &str = "9.8.7";

/// Lists the packages in the directory it is run in, as `apt-ftparchive packages` lists the ones
/// under the path it is given.
const FAKE_APT_FTPARCHIVE: &str = r#"#!/bin/sh
printf '%s\n' "apt-ftparchive $*" >> "$FAKE_LOG"
for argument in "$@"; do last="$argument"; done
case " $* " in
  *" packages "*)
    for file in "$last"/*.deb; do printf 'Filename: ./%s\n\n' "${file##*/}"; done ;;
  *" release "*)
    printf 'Origin: Brave\n'
    for file in "$last"/*; do printf ' hash %s\n' "${file##*/}"; done ;;
esac
"#;

/// Writes the file a client looks for, in the directory it is given.
const FAKE_CREATEREPO: &str = r#"#!/bin/sh
printf '%s\n' "createrepo_c $*" >> "$FAKE_LOG"
mkdir -p "$1/repodata"
printf '<repomd/>\n' > "$1/repodata/repomd.xml"
"#;

/// Stands in for the container runtime: records that it was started and does nothing else.
const FAKE_DOCKER: &str = r#"#!/bin/sh
printf '%s\n' "docker $*" >> "$FAKE_LOG"
"#;

fn workspace() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path
}

fn make_available() -> bool {
    let found = Command::new("make").arg("--version").output().is_ok();
    if !found {
        eprintln!("skipped: make is not on PATH");
    }
    found
}

/// A copy of the Makefile beside a manifest that states a version, with the four packages in
/// `dist/` and the two tools made.
struct Tree {
    root: PathBuf,
}

impl Tree {
    fn new(name: &str) -> Self {
        let root = workspace()
            .join("target/test-scratch/repo")
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("dist")).expect("create scratch");
        std::fs::create_dir_all(root.join("bin")).expect("create bin");
        std::fs::copy(workspace().join("Makefile"), root.join("Makefile")).expect("copy Makefile");
        std::fs::write(
            root.join("Cargo.toml"),
            format!("[workspace.package]\nversion = \"{VERSION}\"\n"),
        )
        .expect("write manifest");
        for arch in ["amd64", "arm64"] {
            for format in ["deb", "rpm"] {
                std::fs::write(
                    root.join(format!("dist/bravebot-linux-{arch}.{format}")),
                    format!("{arch} {format}\n"),
                )
                .expect("write package");
            }
        }
        for (name, body) in [
            ("apt-ftparchive", FAKE_APT_FTPARCHIVE),
            ("createrepo_c", FAKE_CREATEREPO),
            ("docker", FAKE_DOCKER),
        ] {
            let path = root.join("bin").join(name);
            std::fs::write(&path, body).expect("write tool");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("make tool executable");
        }
        Self { root }
    }

    fn make(&self, target: &str) -> Output {
        self.make_with(target, &[])
    }

    fn make_with(&self, target: &str, variables: &[(&str, &Path)]) -> Output {
        let path = format!(
            "{}:{}",
            self.root.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        Command::new("make")
            .arg("-C")
            .arg(&self.root)
            .arg(target)
            .args(
                variables
                    .iter()
                    .map(|(name, value)| format!("{name}={}", value.display())),
            )
            .env("PATH", path)
            .env("FAKE_LOG", self.log())
            .output()
            .expect("run make")
    }

    fn log(&self) -> PathBuf {
        self.root.join("tools.log")
    }

    fn logged(&self) -> String {
        std::fs::read_to_string(self.log()).unwrap_or_default()
    }

    fn names(&self, directory: &str) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.root.join(directory))
            .expect("read directory")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// The apt repository holds both architectures' packages under the names `Packages` points at,
/// the package listing in both forms, and a `Release` that lists them and does not list itself.
///
/// The regression it rejects: writing `Release` into the directory it describes, so that it lists
/// itself with a hash that can never match, or leaving a temporary file beside it; and naming the
/// copied packages without the version, so two releases overwrite each other.
#[test]
fn the_apt_target_writes_a_release_over_both_architectures() {
    if !make_available() {
        return;
    }
    let tree = Tree::new("apt");
    let output = tree.make("cli-repo-metadata-deb");
    assert!(output.status.success(), "{}", stderr(&output));

    assert_eq!(
        tree.names("dist/repo/apt"),
        [
            "Packages".to_owned(),
            "Packages.gz".to_owned(),
            "Release".to_owned(),
            format!("bravebot_{VERSION}_amd64.deb"),
            format!("bravebot_{VERSION}_arm64.deb"),
        ]
    );
    assert_eq!(tree.names("dist/repo"), ["apt".to_owned()]);
    let packages = std::fs::read_to_string(tree.root.join("dist/repo/apt/Packages")).expect("read");
    for arch in ["amd64", "arm64"] {
        assert!(
            packages.contains(&format!("Filename: ./bravebot_{VERSION}_{arch}.deb")),
            "{packages}"
        );
    }
    let release = std::fs::read_to_string(tree.root.join("dist/repo/apt/Release")).expect("read");
    assert!(release.contains("Packages.gz"), "{release}");
    assert!(
        !release
            .lines()
            .any(|line| line.trim_end().ends_with("Release")),
        "Release lists itself: {release}"
    );
    assert!(
        tree.logged().contains("Architectures=amd64 arm64"),
        "{}",
        tree.logged()
    );
}

/// A missing package stops the target before it writes anything or starts the tool, naming the
/// package and the command that makes it.
///
/// The regression it rejects: describing the architectures that are present and publishing a
/// repository that has no package for the other.
#[test]
fn the_apt_target_refuses_when_a_package_is_missing() {
    if !make_available() {
        return;
    }
    let tree = Tree::new("apt-missing");
    std::fs::remove_file(tree.root.join("dist/bravebot-linux-arm64.deb")).expect("remove");
    let output = tree.make("cli-repo-metadata-deb");
    assert!(!output.status.success());
    let said = stderr(&output);
    assert!(said.contains("bravebot-linux-arm64.deb"), "{said}");
    assert!(said.contains("make cli-packages-linux"), "{said}");
    assert!(!tree.root.join("dist/repo").exists());
    assert_eq!(tree.logged(), "");
}

/// The dnf repository holds both architectures' packages under their rpm names, and its metadata
/// is written for that directory.
///
/// The regression it rejects: naming the copies for the Debian architecture (`amd64`), which an
/// rpm client does not match to its own.
#[test]
fn the_rpm_target_writes_repodata_over_both_architectures() {
    if !make_available() {
        return;
    }
    let tree = Tree::new("rpm");
    let output = tree.make("cli-repo-metadata-rpm");
    assert!(output.status.success(), "{}", stderr(&output));

    assert_eq!(
        tree.names("dist/repo/rpm"),
        [
            format!("bravebot-{VERSION}-1.aarch64.rpm"),
            format!("bravebot-{VERSION}-1.x86_64.rpm"),
            "repodata".to_owned(),
        ]
    );
    assert!(
        tree.root
            .join("dist/repo/rpm/repodata/repomd.xml")
            .is_file()
    );
    assert_eq!(tree.logged().trim(), "createrepo_c dist/repo/rpm");
    assert_eq!(tree.names("dist/repo"), ["rpm".to_owned()]);
}

/// A missing package stops the rpm target before it writes anything or starts the tool.
///
/// The regression it rejects: checking the Debian packages only, which the two formats' targets
/// share a loop shape with.
#[test]
fn the_rpm_target_refuses_when_a_package_is_missing() {
    if !make_available() {
        return;
    }
    let tree = Tree::new("rpm-missing");
    std::fs::remove_file(tree.root.join("dist/bravebot-linux-amd64.rpm")).expect("remove");
    let output = tree.make("cli-repo-metadata-rpm");
    assert!(!output.status.success());
    let said = stderr(&output);
    assert!(said.contains("bravebot-linux-amd64.rpm"), "{said}");
    assert!(!tree.root.join("dist/repo").exists());
    assert_eq!(tree.logged(), "");
}

/// The recipes of the three targets hold no key and sign nothing: the signature is supplied by
/// the publish job, so a target that signed would need a key in this tree or in its environment.
#[test]
fn the_metadata_targets_hold_no_signing_step() {
    let makefile = std::fs::read_to_string(workspace().join("Makefile")).expect("read Makefile");
    let start = makefile
        .find("\ncli-repo-metadata:")
        .expect("the wrapper target is in the Makefile");
    let end = makefile[start..]
        .find("\n.PHONY: strip")
        .map(|offset| start + offset)
        .expect("the section ends at the strip target");
    let recipes: String = makefile[start..end]
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    for word in [
        "gpg --",
        "gpgv",
        "--sign",
        "--clearsign",
        "--detach",
        "rpmsign",
        "SIGNING",
    ] {
        assert!(!recipes.contains(word), "{word} in the metadata recipes");
    }
}

/// Signature files the publish job made, with bytes that differ from each other and are not text,
/// so a copy that swaps two of them or re-encodes one is seen.
fn supplied(tree: &Tree, name: &str, bytes: &[u8]) -> PathBuf {
    let path = tree.root.join("supplied").join(name);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create directory");
    std::fs::write(&path, bytes).expect("write signature");
    path
}

const INRELEASE: &[u8] = b"-----BEGIN PGP SIGNED MESSAGE-----\n\xff\xfe clearsigned\n";
const RELEASE_GPG: &[u8] = b"\x89\x01\x00 detached over Release";
const REPOMD_ASC: &[u8] = b"-----BEGIN PGP SIGNATURE-----\n\xfe\xff over repomd\n";

/// A signature file named for the apt repository is copied, byte for byte, to the name apt looks
/// for, and the two apt signatures stay apart.
///
/// The regression it rejects: copying the wrong input to a name (`InRelease` from the detached
/// signature), re-encoding the bytes, or placing a file only when the other is also set.
#[test]
fn the_apt_target_places_the_signatures_it_is_given() {
    if !make_available() {
        return;
    }
    let tree = Tree::new("apt-signed");
    let inrelease = supplied(&tree, "inrelease", INRELEASE);
    let release_gpg = supplied(&tree, "release-gpg", RELEASE_GPG);

    let output = tree.make_with(
        "cli-repo-metadata-deb",
        &[
            ("REPO_APT_INRELEASE", &inrelease),
            ("REPO_APT_RELEASE_GPG", &release_gpg),
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let apt = tree.root.join("dist/repo/apt");
    assert_eq!(
        std::fs::read(apt.join("InRelease")).expect("read"),
        INRELEASE
    );
    assert_eq!(
        std::fs::read(apt.join("Release.gpg")).expect("read"),
        RELEASE_GPG
    );

    let only_detached = Tree::new("apt-detached");
    let release_gpg = supplied(&only_detached, "release-gpg", RELEASE_GPG);
    let output = only_detached.make_with(
        "cli-repo-metadata-deb",
        &[("REPO_APT_RELEASE_GPG", &release_gpg)],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let names = only_detached.names("dist/repo/apt");
    assert!(names.contains(&"Release.gpg".to_owned()), "{names:?}");
    assert!(!names.contains(&"InRelease".to_owned()), "{names:?}");
}

/// A signature file named for the dnf repository is copied, byte for byte, beside `repomd.xml`.
///
/// The regression it rejects: placing it beside the packages instead of in `repodata/`, where dnf
/// does not look.
#[test]
fn the_rpm_target_places_the_signature_it_is_given() {
    if !make_available() {
        return;
    }
    let tree = Tree::new("rpm-signed");
    let asc = supplied(&tree, "repomd-asc", REPOMD_ASC);
    let output = tree.make_with("cli-repo-metadata-rpm", &[("REPO_RPM_REPOMD_ASC", &asc)]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        tree.names("dist/repo/rpm/repodata"),
        ["repomd.xml".to_owned(), "repomd.xml.asc".to_owned()]
    );
    assert_eq!(
        std::fs::read(tree.root.join("dist/repo/rpm/repodata/repomd.xml.asc")).expect("read"),
        REPOMD_ASC
    );
}

/// A signature variable set to a path that is not a file stops the target, naming the variable,
/// before any tool runs and before anything is written. The wrapper stops before it starts a
/// container, and does so for a variable only the other format uses.
///
/// The regression it rejects: finding out only when `cp` fails, after `apt-ftparchive` or
/// `createrepo_c` has run and the repository has been rewritten without the signature, and a
/// wrapper that builds the apt repository in a container before it notices.
#[test]
fn a_signature_path_that_is_not_a_file_stops_the_target_before_a_tool_runs() {
    if !make_available() {
        return;
    }
    for (target, variable) in [
        ("cli-repo-metadata-deb", "REPO_APT_INRELEASE"),
        ("cli-repo-metadata-deb", "REPO_APT_RELEASE_GPG"),
        ("cli-repo-metadata-rpm", "REPO_RPM_REPOMD_ASC"),
        ("cli-repo-metadata", "REPO_APT_INRELEASE"),
        ("cli-repo-metadata", "REPO_APT_RELEASE_GPG"),
        ("cli-repo-metadata", "REPO_RPM_REPOMD_ASC"),
    ] {
        let tree = Tree::new("signature-missing");
        let absent = tree.root.join("supplied/absent");
        let directory = tree.root.join("dist");
        for path in [&absent, &directory] {
            let output = tree.make_with(target, &[(variable, path)]);
            assert!(!output.status.success(), "{target} {variable}");
            let said = stderr(&output);
            assert!(said.contains(variable), "{target}: {said}");
            assert!(said.contains("not a file"), "{target}: {said}");
            assert!(!tree.root.join("dist/repo").exists(), "{target} {variable}");
            assert_eq!(tree.logged(), "", "{target} {variable}");
        }
    }
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(workspace().join(path)).expect("read file")
}

/// The README and the install page give a client the key by its published location, tell apt which
/// key signs the repository, and tell dnf to check the repository's metadata, and neither turns a
/// check off.
///
/// The regression it rejects: an instruction that works by trusting the repository unchecked
/// (`trusted=yes`, `repo_gpgcheck=0`, `--nogpgcheck`, `sslverify=0`), which installs fine and
/// leaves a client believing anything the host serves.
#[test]
fn the_install_instructions_check_the_repository_metadata() {
    for file in ["README.md", "docs/website/docs/install.md"] {
        let text = read(Path::new(file));
        for needed in [
            "Signed-By: /usr/share/keyrings/bravebot-release.asc",
            "repo_gpgcheck=1",
            "gpgkey=https://brave-browser-downloads.s3.brave.com/keys/bravebot-release.asc",
            "https://brave-browser-downloads.s3.brave.com/bravebot/apt/",
            "https://brave-browser-downloads.s3.brave.com/bravebot/rpm/",
        ] {
            assert!(text.contains(needed), "{file} lacks {needed}");
        }
        for refused in [
            "trusted=yes",
            "repo_gpgcheck=0",
            "--nogpgcheck",
            "sslverify=0",
            "allow-insecure",
        ] {
            assert!(!text.contains(refused), "{file} says {refused}");
        }
    }
}
