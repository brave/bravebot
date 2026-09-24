//! What each installer does with the signature published beside a Linux checksum, observed by
//! running the installer itself against a release served from a directory.
//!
//! [RELEASE-13] is the clause. Both installers are shell and JavaScript, so nothing in this
//! workspace calls them, and running them is the only way a Rust test can hold them to it. Nothing
//! is fetched from the network: `install.sh` finds a `curl` made here first on its `PATH`, and the
//! npm installer's requests are answered from the same directory. The release key, and the key of
//! somebody replacing the release, are both made here, and each installer is pointed at the first
//! by fingerprint, as the published installers are pointed at Brave's.
//!
//! Linux only, because the Linux path is what is under test and a macOS checkout's path is long
//! enough to overrun the socket name limit of a keyring kept beside it. Skipped where `gpg`, `node`
//! or `sha256sum` is not on `PATH`, and says so.
//!
//! [RELEASE-13]: ../../../docs/specs/releases.md

#![cfg(target_os = "linux")]

use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// What `install.sh` runs, beside `curl` and `uname`, which are made here instead.
const UTILITIES: &[&str] = &[
    "cat",
    "chmod",
    "cut",
    "dirname",
    "grep",
    "head",
    "mkdir",
    "mktemp",
    "mv",
    "rm",
    "sed",
    "sha256sum",
    "tr",
];

/// Serves a release from `$SERVED` by the last segment of the URL, and fails as `curl -f` does on
/// a missing file. The newest-release lookup gets a tag of its own.
const FAKE_CURL: &str = r#"#!/bin/sh
out=""
url=""
while [ $# -gt 0 ]; do
  case "$1" in
    -o) out="$2"; shift 2 ;;
    -*) shift ;;
    *) url="$1"; shift ;;
  esac
done
case "$url" in
  */releases/latest) printf '{"tag_name": "v0.0.0"}\n'; exit 0 ;;
esac
file="$SERVED/${url##*/}"
[ -f "$file" ] || exit 22
if [ -n "$out" ]; then cat "$file" > "$out"; else cat "$file"; fi
"#;

const FAKE_UNAME: &str = r#"#!/bin/sh
case "$1" in
  -s) echo "$FAKE_OS" ;;
  -m) echo aarch64 ;;
esac
"#;

/// Answers the npm installer's requests from the served directory, then installs.
///
/// The installer reads `https.get` off the shared module each time it fetches, so replacing it
/// before the installer is loaded is enough to keep every request on this machine.
const NODE_HARNESS: &str = r#"
const fs = require("node:fs");
const path = require("node:path");
const https = require("node:https");
const { Readable } = require("node:stream");
const { EventEmitter } = require("node:events");
const [served, script, asset, destination, fingerprint] = process.argv.slice(1);
https.get = (url, options, callback) => {
  const file = path.join(served, new URL(url).pathname.split("/").pop());
  const found = fs.existsSync(file);
  const response = Readable.from(found ? [fs.readFileSync(file)] : []);
  response.statusCode = found ? 200 : 404;
  response.headers = {};
  process.nextTick(callback, response);
  return new EventEmitter();
};
require(script)
  .install("v0.0.0", { asset, binaryName: "bravebot-bin" }, "https://release.invalid/download", destination, fingerprint)
  .then(
    () => process.exit(0),
    (error) => {
      console.error(error.message);
      process.exit(1);
    },
  );
"#;

/// The programs these tests run, found on the `PATH` the test runner was given.
struct Tools {
    sh: PathBuf,
    node: PathBuf,
    gpg: PathBuf,
    gpgconf: Option<PathBuf>,
    utilities: Vec<(&'static str, PathBuf)>,
}

fn on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

fn tools() -> Option<&'static Tools> {
    static TOOLS: OnceLock<Option<Tools>> = OnceLock::new();
    TOOLS
        .get_or_init(|| {
            let mut utilities = Vec::new();
            for name in UTILITIES {
                utilities.push((*name, on_path(name)?));
            }
            Some(Tools {
                sh: on_path("sh")?,
                node: on_path("node")?,
                gpg: on_path("gpg")?,
                gpgconf: on_path("gpgconf"),
                utilities,
            })
        })
        .as_ref()
}

/// Everything a published release is made of, twice: once as Brave publishes it, and once as
/// somebody replacing the binary would, with a checksum that matches their binary.
struct Release {
    binary: Vec<u8>,
    checksum: Vec<u8>,
    signature: Vec<u8>,
    substitute: Vec<u8>,
    substitute_checksum: Vec<u8>,
    /// Over the substitute's checksum, by a key that is not the release key.
    substitute_signature: Vec<u8>,
    release_key: Vec<u8>,
    /// The release key and the substitute's signer in one file, as a key host somebody has added
    /// to would serve.
    both_keys: Vec<u8>,
    fingerprint: String,
}

fn workspace() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path
}

/// Under this workspace's build directory rather than the system temporary one, which is shared
/// between users and where a name this predictable is somebody else's to create first. Kept short,
/// since a keyring's sockets are made inside it and a socket's path has a length limit.
fn scratch_root() -> PathBuf {
    workspace().join("target/test-scratch/sig")
}

fn fresh(path: &Path) {
    let _ = std::fs::remove_dir_all(path);
    std::fs::create_dir_all(path).expect("create scratch");
}

fn gpg(tools: &Tools, home: &Path, arguments: &[&str]) -> Vec<u8> {
    let output = Command::new(&tools.gpg)
        .arg("--homedir")
        .arg(home)
        .args(["--batch", "--pinentry-mode", "loopback", "--passphrase", ""])
        .args(arguments)
        .output()
        .expect("run gpg");
    assert!(
        output.status.success(),
        "gpg {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn fingerprint_of(tools: &Tools, home: &Path, uid: &str) -> String {
    let listing = gpg(tools, home, &["--with-colons", "--fingerprint", uid]);
    String::from_utf8(listing)
        .expect("utf-8 listing")
        .lines()
        .find_map(|line| line.strip_prefix("fpr:"))
        .and_then(|rest| rest.split(':').nth(8))
        .expect("a fingerprint line")
        .to_owned()
}

fn checksum(tools: &Tools, bytes: &[u8]) -> Vec<u8> {
    let path = scratch_root().join("keys/digest-input");
    std::fs::write(&path, bytes).expect("write digest input");
    let sha256sum = tools
        .utilities
        .iter()
        .find(|(name, _)| *name == "sha256sum")
        .map(|(_, path)| path)
        .expect("sha256sum was resolved");
    let output = Command::new(sha256sum)
        .arg(&path)
        .output()
        .expect("run sha256sum");
    let digest = String::from_utf8(output.stdout).expect("utf-8 digest");
    format!("{}\n", digest.split_whitespace().next().expect("a digest")).into_bytes()
}

/// Made once for every test here, then the keyring that made it is removed, so no agent is left
/// holding a secret key once the fixtures exist.
fn release(tools: &Tools) -> &'static Release {
    static RELEASE: OnceLock<Release> = OnceLock::new();
    RELEASE.get_or_init(|| {
        let home = scratch_root().join("keys");
        fresh(&home);
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700))
            .expect("narrow the keyring");

        let release_uid = "Release <release@example.invalid>";
        let other_uid = "Substitute <substitute@example.invalid>";
        for uid in [release_uid, other_uid] {
            gpg(
                tools,
                &home,
                &["--quick-gen-key", uid, "ed25519", "sign", "never"],
            );
        }
        let fingerprint = fingerprint_of(tools, &home, release_uid);
        let other = fingerprint_of(tools, &home, other_uid);

        let sign = |signer: &str, checksum: &[u8]| {
            let input = home.join("checksum");
            std::fs::write(&input, checksum).expect("write checksum");
            let signer = format!("{signer}!");
            let input = input.to_str().expect("utf-8 path");
            gpg(
                tools,
                &home,
                &[
                    "--armor",
                    "--local-user",
                    &signer,
                    "--detach-sign",
                    "--output",
                    "-",
                    input,
                ],
            )
        };

        let binary = b"#!/bin/sh\necho released\n".to_vec();
        let substitute = b"#!/bin/sh\necho substituted\n".to_vec();
        let checksum_of_binary = checksum(tools, &binary);
        let substitute_checksum = checksum(tools, &substitute);
        let release = Release {
            signature: sign(&fingerprint, &checksum_of_binary),
            substitute_signature: sign(&other, &substitute_checksum),
            release_key: gpg(tools, &home, &["--armor", "--export", &fingerprint]),
            both_keys: gpg(tools, &home, &["--armor", "--export", &fingerprint, &other]),
            binary,
            checksum: checksum_of_binary,
            substitute,
            substitute_checksum,
            fingerprint,
        };

        if let Some(gpgconf) = &tools.gpgconf {
            let _ = Command::new(gpgconf)
                .arg("--homedir")
                .arg(&home)
                .args(["--kill", "all"])
                .status();
        }
        let _ = std::fs::remove_dir_all(&home);
        release
    })
}

fn setup() -> Option<(&'static Tools, &'static Release)> {
    let Some(tools) = tools() else {
        eprintln!("skipped: gpg, node, sh or one of {UTILITIES:?} is not on PATH");
        return None;
    };
    Some((tools, release(tools)))
}

#[derive(Clone, Copy)]
enum Platform {
    Linux,
    Darwin,
    Windows,
}

#[derive(Clone, Copy, Debug)]
enum Installer {
    Script,
    Npm,
}

/// One install, run in directories nobody else is using and removed afterwards.
struct Install<'a> {
    tools: &'a Tools,
    fingerprint: &'a str,
    root: PathBuf,
    platform: Platform,
    gpg_on_path: bool,
}

/// What an install left behind.
struct Outcome {
    succeeded: bool,
    output: String,
    /// The bytes written where the executable goes, if anything was.
    installed: Option<Vec<u8>>,
    /// Anything left in the temporary directory the installer was given, where its keyring is made.
    left_in_temporary: Vec<PathBuf>,
    /// Anything written to the keyring the environment names as the person's own.
    left_in_own_keyring: Vec<PathBuf>,
}

impl<'a> Install<'a> {
    fn new(tools: &'a Tools, release: &'a Release, name: &str) -> Self {
        let root = scratch_root().join(name);
        fresh(&root);
        Self {
            tools,
            fingerprint: &release.fingerprint,
            root,
            platform: Platform::Linux,
            gpg_on_path: true,
        }
    }

    fn on(mut self, platform: Platform) -> Self {
        self.platform = platform;
        self
    }

    fn without_gpg(mut self) -> Self {
        self.gpg_on_path = false;
        self
    }

    fn asset(&self) -> &'static str {
        match self.platform {
            Platform::Linux => "bravebot-linux-arm64",
            Platform::Darwin => "bravebot-darwin-arm64",
            Platform::Windows => "bravebot-windows-arm64.exe",
        }
    }

    fn directory(&self, installer: Installer, name: &str) -> PathBuf {
        let path = self.root.join(format!("{installer:?}")).join(name);
        std::fs::create_dir_all(&path).expect("create install directory");
        path
    }

    /// A `PATH` holding only what the installer needs, so `gpg` is on it exactly when asked for.
    fn path(&self, installer: Installer) -> PathBuf {
        let directory = self.directory(installer, "path");
        for (name, target) in &self.tools.utilities {
            symlink(target, directory.join(name)).expect("link a utility");
        }
        if self.gpg_on_path {
            symlink(&self.tools.gpg, directory.join("gpg")).expect("link gpg");
        }
        for (name, body) in [("curl", FAKE_CURL), ("uname", FAKE_UNAME)] {
            let path = directory.join(name);
            std::fs::write(&path, body).expect("write a stand-in");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("make a stand-in executable");
        }
        directory
    }

    /// Run one installer against a release made of `files`, each named as its URL ends.
    fn run(&self, installer: Installer, files: &[(&str, &[u8])]) -> Outcome {
        let served = self.directory(installer, "served");
        for (name, bytes) in files {
            std::fs::write(served.join(name), bytes).expect("serve a file");
        }
        let temporary = self.directory(installer, "tmp");
        let own_keyring = self.directory(installer, "own-gnupg");
        let home = self.directory(installer, "home");
        let bin = self.directory(installer, "bin");
        let path = self.path(installer);

        let (mut command, destination) = match installer {
            Installer::Script => {
                let mut command = Command::new(&self.tools.sh);
                command
                    .env_clear()
                    .arg("-c")
                    .arg(
                        r#"fingerprint="$2"; . "$1"; SIGNING_KEY_FINGERPRINT="$fingerprint"; main"#,
                    )
                    .arg("sh")
                    .arg(workspace().join("install.sh"))
                    .arg(self.fingerprint)
                    .env("BRAVEBOT_INSTALL_SH_TEST", "1")
                    .env("INSTALL_DIR", &bin)
                    .env("SERVED", &served)
                    .env(
                        "FAKE_OS",
                        match self.platform {
                            Platform::Linux => "Linux",
                            Platform::Darwin => "Darwin",
                            Platform::Windows => {
                                unreachable!("the script does not install Windows")
                            }
                        },
                    );
                (command, bin.join("bravebot"))
            }
            Installer::Npm => {
                let destination = bin.join("bravebot-bin");
                let mut command = Command::new(&self.tools.node);
                command
                    .env_clear()
                    .arg("-e")
                    .arg(NODE_HARNESS)
                    .arg(&served)
                    .arg(workspace().join("npm/scripts/postinstall.js"))
                    .arg(self.asset())
                    .arg(&destination)
                    .arg(self.fingerprint);
                (command, destination)
            }
        };
        let output = command
            .env("PATH", &path)
            .env("HOME", &home)
            .env("TMPDIR", &temporary)
            .env("GNUPGHOME", &own_keyring)
            .output()
            .expect("run the installer");

        let entries = |directory: &Path| -> Vec<PathBuf> {
            std::fs::read_dir(directory)
                .expect("read directory")
                .map(|entry| entry.expect("directory entry").path())
                .collect()
        };
        Outcome {
            succeeded: output.status.success(),
            output: format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
            installed: std::fs::read(&destination).ok(),
            left_in_temporary: entries(&temporary),
            left_in_own_keyring: entries(&own_keyring),
        }
    }

    /// Run both installers, or only the npm one where the script does not install the platform.
    fn each(&self, files: &[(&str, &[u8])]) -> Vec<(Installer, Outcome)> {
        let installers: &[Installer] = match self.platform {
            Platform::Windows => &[Installer::Npm],
            Platform::Linux | Platform::Darwin => &[Installer::Script, Installer::Npm],
        };
        installers
            .iter()
            .map(|installer| (*installer, self.run(*installer, files)))
            .collect()
    }
}

impl Drop for Install<'_> {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Refused, writing nothing, and for the reason given: a refusal the fixture caused by accident is
/// not evidence that the installer refuses what the test changed.
fn assert_refused(installer: Installer, outcome: &Outcome, because: &str) {
    assert!(
        !outcome.succeeded,
        "{installer:?} installed: {}",
        outcome.output
    );
    assert_eq!(outcome.installed, None, "{installer:?} wrote an executable");
    assert!(
        outcome.output.to_lowercase().contains(because),
        "{installer:?} refused for a reason other than {because:?}: {}",
        outcome.output
    );
    assert_eq!(
        outcome.left_in_temporary,
        Vec::<PathBuf>::new(),
        "{installer:?} left its keyring behind"
    );
}

fn assert_installed(installer: Installer, outcome: &Outcome, binary: &[u8]) {
    assert!(
        outcome.succeeded,
        "{installer:?} refused: {}",
        outcome.output
    );
    assert_eq!(
        outcome.installed.as_deref(),
        Some(binary),
        "{installer:?} installed something other than the release"
    );
    assert_eq!(
        outcome.left_in_temporary,
        Vec::<PathBuf>::new(),
        "{installer:?} left its keyring behind"
    );
}

/// The release as published installs, and the check leaves nothing in the person's own keyring.
///
/// Every refusal below is a change to this release, so this is what shows the rest of it installs.
#[test]
fn a_linux_checksum_signed_by_the_release_key_installs() {
    let Some((tools, release)) = setup() else {
        return;
    };
    let install = Install::new(tools, release, "signed");
    for (installer, outcome) in install.each(&[
        ("bravebot-linux-arm64", &release.binary),
        ("bravebot-linux-arm64.sha256", &release.checksum),
        ("bravebot-linux-arm64.sha256.asc", &release.signature),
        ("bravebot-release.asc", &release.release_key),
    ]) {
        assert_installed(installer, &outcome, &release.binary);
        assert_eq!(
            outcome.left_in_own_keyring,
            Vec::<PathBuf>::new(),
            "{installer:?} wrote to the person's own keyring"
        );
    }
}

/// Replacing the binary and its checksum together is what the checksum alone cannot catch, since
/// both are uploaded to the same place. The signature is over the checksum that was replaced.
#[test]
fn a_checksum_replaced_after_it_was_signed_installs_nothing() {
    let Some((tools, release)) = setup() else {
        return;
    };
    let install = Install::new(tools, release, "replaced");
    for (installer, outcome) in install.each(&[
        ("bravebot-linux-arm64", &release.substitute),
        ("bravebot-linux-arm64.sha256", &release.substitute_checksum),
        ("bravebot-linux-arm64.sha256.asc", &release.signature),
        ("bravebot-release.asc", &release.release_key),
    ]) {
        assert_refused(installer, &outcome, "signature verification failed");
    }
}

/// Somebody who can change both the release and the key host can sign their own checksum and
/// serve their own key beside Brave's. The key file served here holds the release key too, so an
/// installer that only checked the file contained it would install the substitute.
#[test]
fn a_checksum_signed_by_any_key_but_the_release_key_installs_nothing() {
    let Some((tools, release)) = setup() else {
        return;
    };
    let install = Install::new(tools, release, "other-key");
    for (installer, outcome) in install.each(&[
        ("bravebot-linux-arm64", &release.substitute),
        ("bravebot-linux-arm64.sha256", &release.substitute_checksum),
        (
            "bravebot-linux-arm64.sha256.asc",
            &release.substitute_signature,
        ),
        ("bravebot-release.asc", &release.both_keys),
    ]) {
        assert_refused(installer, &outcome, "signature verification failed");
    }
}

/// Deleting the signature is the cheapest way to get past a check that skips a missing one, and
/// it is available to exactly the person the check is for.
#[test]
fn a_linux_checksum_with_no_signature_installs_nothing() {
    let Some((tools, release)) = setup() else {
        return;
    };
    let install = Install::new(tools, release, "unsigned");
    for (installer, outcome) in install.each(&[
        ("bravebot-linux-arm64", &release.binary),
        ("bravebot-linux-arm64.sha256", &release.checksum),
        ("bravebot-release.asc", &release.release_key),
    ]) {
        let because = match installer {
            Installer::Script => "checksum signature",
            Installer::Npm => "bravebot-linux-arm64.sha256.asc",
        };
        assert_refused(installer, &outcome, because);
    }
}

/// A key that cannot be fetched leaves nothing to check the signature against, which is no more a
/// reason to install than a signature that is missing.
#[test]
fn a_release_key_that_cannot_be_fetched_installs_nothing() {
    let Some((tools, release)) = setup() else {
        return;
    };
    let install = Install::new(tools, release, "no-key");
    for (installer, outcome) in install.each(&[
        ("bravebot-linux-arm64", &release.binary),
        ("bravebot-linux-arm64.sha256", &release.checksum),
        ("bravebot-linux-arm64.sha256.asc", &release.signature),
    ]) {
        assert_refused(installer, &outcome, "bravebot-release.asc");
    }
}

/// Whether `gpg` is installed is a property of the machine, which nobody changing a release
/// controls, so its absence is no reason to refuse. It is said, since the install is then on the
/// checksum alone. Nothing but the binary and its checksum is served, so an installer that fetched
/// the signature anyway would fail here.
#[test]
fn without_gpg_a_linux_install_says_it_skipped_the_signature_and_installs() {
    let Some((tools, release)) = setup() else {
        return;
    };
    let install = Install::new(tools, release, "no-gpg").without_gpg();
    for (installer, outcome) in install.each(&[
        ("bravebot-linux-arm64", &release.binary),
        ("bravebot-linux-arm64.sha256", &release.checksum),
    ]) {
        assert_installed(installer, &outcome, &release.binary);
        assert!(
            outcome.output.contains("skipping signature verification"),
            "{installer:?} did not say it skipped the check: {}",
            outcome.output
        );
    }
}

/// Darwin and Windows binaries carry a signature of their own and no signature is published beside
/// their checksums, so an installer that asked for one there would fail every install on them.
#[test]
fn a_darwin_or_windows_install_neither_fetches_nor_needs_a_signature() {
    let Some((tools, release)) = setup() else {
        return;
    };
    for (platform, asset, checksum) in [
        (
            Platform::Darwin,
            "bravebot-darwin-arm64",
            "bravebot-darwin-arm64.sha256",
        ),
        (
            Platform::Windows,
            "bravebot-windows-arm64.exe",
            "bravebot-windows-arm64.exe.sha256",
        ),
    ] {
        let install = Install::new(tools, release, asset).on(platform);
        for (installer, outcome) in
            install.each(&[(asset, &release.binary), (checksum, &release.checksum)])
        {
            assert_installed(installer, &outcome, &release.binary);
        }
    }
}
