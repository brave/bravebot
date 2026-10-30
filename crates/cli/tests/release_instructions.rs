//! What the README and the install page tell a person to run to install the CLI from the apt and
//! dnf repositories.
//!
//! [RELEASE-22] is the clause. The repositories are generated, signed and uploaded by the publish
//! job, which this tree does not contain, so the text is what is held here: the two files name the
//! served locations and the release key, and neither turns a check off. Whether `apt` and `dnf`
//! accept the served repositories is not checked here.
//!
//! [RELEASE-22]: ../../../docs/specs/releases.md

use std::path::Path;

fn read(file: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::fs::read_to_string(root.join(file)).expect("read file")
}

/// The README and the install page give a client the served repositories, the release key and the
/// served files that carry the client's settings, and neither turns a check off. The install page
/// also gives the commands that install and remove the package.
///
/// The regression it rejects: an instruction that works by trusting the repository unchecked
/// (`trusted=yes`, `gpgcheck=0`, `repo_gpgcheck=0`, `--nogpgcheck`, `sslverify=0`), which installs
/// fine and leaves a client believing anything the host serves, or one that names a host the
/// repositories are not published at.
#[test]
fn the_install_instructions_name_the_served_repositories() {
    for file in ["README.md", "docs/website/docs/install.md"] {
        let text = read(file);
        for needed in [
            "https://bravebot-apt-release.s3.brave.com",
            "https://bravebot-rpm-release.s3.brave.com",
            "13F28F0405C49B0B232DBA1BC1E827646A2DE416",
            "bravebot-archive-keyring.gpg",
            "https://bravebot-apt-release.s3.brave.com/bravebot.sources",
            "https://bravebot-rpm-release.s3.brave.com/bravebot.repo",
        ] {
            assert!(text.contains(needed), "{file} lacks {needed}");
        }
        for refused in [
            "trusted=yes",
            "repo_gpgcheck=0",
            "--nogpgcheck",
            "sslverify=0",
            "allow-insecure",
            "gpgcheck=0",
            "brave-browser-downloads.s3.brave.com/bravebot",
        ] {
            assert!(!text.contains(refused), "{file} says {refused}");
        }
    }

    let page = read("docs/website/docs/install.md");
    for needed in [
        "sudo apt install bravebot",
        "sudo dnf install bravebot",
        "sudo apt remove bravebot",
        "sudo dnf remove bravebot",
    ] {
        assert!(page.contains(needed), "the install page lacks {needed}");
    }
}
