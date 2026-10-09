//! What each installer does with the signature published beside a Linux checksum, and with the
//! code signature a macOS or Windows binary carries, observed by running the installer itself
//! against a release served from a directory.
//!
//! [RELEASE-14] is the clause for Linux and [RELEASE-15] the one for macOS and Windows. Both installers are shell and JavaScript, so nothing in this
//! workspace calls them, and running them is the only way a Rust test can hold them to it. Nothing
//! is fetched from the network: `install.sh` finds a `curl` made here first on its `PATH`, and the
//! npm installer's requests are answered from the same directory. The release key, and the key of
//! somebody replacing the release, are both made here; each installer is handed the first as the
//! key it carries and pointed at it by fingerprint, as the published installers carry and name
//! Brave's.
//!
//! `codesign`, `spctl` and `powershell.exe` are made here too, and stand in for the real ones in
//! the way a real one answers for a binary: `codesign` reads the team a binary says it is signed by
//! from a marker line in it, and refuses when the requirement it is given names another team. What
//! the real tools say about Brave's binaries was checked by hand and is not something these tests
//! can show.
//!
//! Linux only, because the Linux path is what is under test and a macOS checkout's path is long
//! enough to overrun the socket name limit of a keyring kept beside it. Skipped where `gpg`, `node`
//! or `sha256sum` is not on `PATH`, and says so.
//!
//! [RELEASE-14]: ../../../docs/specs/releases.md
//! [RELEASE-15]: ../../../docs/specs/releases.md

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
[ -z "$REQUESTS" ] || printf '%s\n' "$url" >> "$REQUESTS"
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

/// Checks a signature as `codesign --verify -R` does: a binary that says nothing about who signed
/// it is unsigned, and a requirement that names a team other than the binary's is not satisfied.
/// Without a requirement, any team's signature is valid, which is how the real tool behaves.
const FAKE_CODESIGN: &str = r#"#!/bin/sh
requirement=""
file=""
while [ $# -gt 0 ]; do
  case "$1" in
    -R) requirement="$2"; shift 2 ;;
    -*) shift ;;
    *) file="$1"; shift ;;
  esac
done
[ -f "$file" ] || { echo "codesign: $file: No such file" >&2; exit 1; }
team="$(sed -n 's/^# signed-by: //p' "$file")"
[ -n "$team" ] || { echo "$file: code object is not signed at all" >&2; exit 1; }
if [ -n "$requirement" ]; then
  case "$requirement" in
    *"\"$team\""*) ;;
    *) echo "$file: does not satisfy its designated Requirement" >&2; exit 3 ;;
  esac
fi
"#;

/// Assesses as Gatekeeper does for an installer package: a bare executable is only accepted as
/// `-t install`, and only when the binary says it was notarized.
const FAKE_SPCTL: &str = r#"#!/bin/sh
type=""
file=""
while [ $# -gt 0 ]; do
  case "$1" in
    -t) type="$2"; shift 2 ;;
    -*) shift ;;
    *) file="$1"; shift ;;
  esac
done
[ "$type" = install ] || { echo "$file: rejected (the code is valid but does not seem to be an app)" >&2; exit 3; }
grep -q '^# notarized$' "$file" || { echo "$file: rejected" >&2; exit 3; }
"#;

/// Reports what `FAKE_AUTH_STATUS` and `FAKE_AUTH_SIGNER` say, as the script the installer sends
/// prints a status and then a signer, and fails when it is not given the flags it should be.
const FAKE_POWERSHELL: &str = r#"#!/bin/sh
[ "$1" = "-NoProfile" ] && [ "$2" = "-NonInteractive" ] && [ "$3" = "-EncodedCommand" ] && [ -n "$4" ] || exit 64
printf '%s\r\n%s\r\n' "$FAKE_AUTH_STATUS" "$FAKE_AUTH_SIGNER"
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
const [served, script, asset, platform, destination, fingerprint, key] = process.argv.slice(1);
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
  .install({
    target: { asset, platform, binaryName: "bravebot-bin" },
    tag: "v0.0.0",
    destination,
    fingerprint,
    publicKey: key,
  })
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

/// The keyring the fixtures are made in, one per process. `release` runs once per process, and a
/// test runner that gives each test a process of its own would otherwise have every one of them
/// clearing the same keyring and stopping the same agent while the others were still using it.
fn keys_home() -> PathBuf {
    scratch_root().join(format!("keys-{}", std::process::id()))
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
    let path = keys_home().join("digest-input");
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
        let home = keys_home();
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
    release_key: &'a str,
    root: PathBuf,
    platform: Platform,
    gpg_on_path: bool,
    /// Programs the installer looks for by name that are left off its `PATH`.
    absent: Vec<&'static str>,
    /// What the stand-in for `powershell.exe` reports.
    authenticode: (&'static str, &'static str),
    /// What the script is given after its name, as a person writes `sh -s 1.2.3`.
    arguments: Vec<&'static str>,
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
    /// Anything in the directory the executable goes to besides the executable, where the npm
    /// installer stages a download before it has been checked.
    left_beside_destination: Vec<PathBuf>,
    /// Every URL the script asked `curl` for, in order.
    requests: Vec<String>,
}

impl<'a> Install<'a> {
    fn new(tools: &'a Tools, release: &'a Release, name: &str) -> Self {
        let root = scratch_root().join(name);
        fresh(&root);
        Self {
            tools,
            fingerprint: &release.fingerprint,
            release_key: std::str::from_utf8(&release.release_key).expect("utf-8 release key"),
            root,
            platform: Platform::Linux,
            gpg_on_path: true,
            absent: Vec::new(),
            authenticode: ("Valid", "Brave Software, Inc."),
            arguments: Vec::new(),
        }
    }

    fn with_arguments(mut self, arguments: &[&'static str]) -> Self {
        self.arguments = arguments.to_vec();
        self
    }

    fn on(mut self, platform: Platform) -> Self {
        self.platform = platform;
        self
    }

    fn without_gpg(mut self) -> Self {
        self.gpg_on_path = false;
        self
    }

    fn without(mut self, program: &'static str) -> Self {
        self.absent.push(program);
        self
    }

    fn authenticode(mut self, status: &'static str, signer: &'static str) -> Self {
        self.authenticode = (status, signer);
        self
    }

    fn node_platform(&self) -> &'static str {
        match self.platform {
            Platform::Linux => "linux",
            Platform::Darwin => "darwin",
            Platform::Windows => "win32",
        }
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
        for (name, body) in [
            ("curl", FAKE_CURL),
            ("uname", FAKE_UNAME),
            ("codesign", FAKE_CODESIGN),
            ("spctl", FAKE_SPCTL),
            ("powershell.exe", FAKE_POWERSHELL),
        ] {
            if self.absent.contains(&name) {
                continue;
            }
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
        let requests = self.directory(installer, "log").join("requests");

        let (mut command, destination) = match installer {
            Installer::Script => {
                let mut command = Command::new(&self.tools.sh);
                command
                    .env_clear()
                    .arg("-c")
                    .arg(
                        r#"fingerprint="$2"; key="$3"; . "$1"; SIGNING_KEY_FINGERPRINT="$fingerprint"; RELEASE_PUBKEY="$key"; shift 3; main "$@""#,
                    )
                    .arg("sh")
                    .arg(workspace().join("install.sh"))
                    .arg(self.fingerprint)
                    .arg(self.release_key)
                    .args(&self.arguments)
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
                    .arg(self.node_platform())
                    .arg(&destination)
                    .arg(self.fingerprint)
                    .arg(self.release_key);
                (command, destination)
            }
        };
        let output = command
            .env("PATH", &path)
            .env("HOME", &home)
            .env("TMPDIR", &temporary)
            .env("GNUPGHOME", &own_keyring)
            .env("REQUESTS", &requests)
            .env("FAKE_AUTH_STATUS", self.authenticode.0)
            .env("FAKE_AUTH_SIGNER", self.authenticode.1)
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
            left_beside_destination: entries(&bin)
                .into_iter()
                .filter(|path| *path != destination)
                .collect(),
            requests: std::fs::read_to_string(&requests)
                .unwrap_or_default()
                .lines()
                .map(str::to_owned)
                .collect(),
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
    assert_eq!(
        outcome.left_beside_destination,
        Vec::<PathBuf>::new(),
        "{installer:?} left a download beside the destination"
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
    assert_eq!(
        outcome.left_beside_destination,
        Vec::<PathBuf>::new(),
        "{installer:?} left a download beside the destination"
    );
}

/// The key each installer carries is the one it names, and both carry the same one. Every test
/// below hands the installer a key instead of letting it read its own, so this is the one place
/// the embedded key is read, and the two files that hold it are held to each other.
#[test]
fn both_installers_embed_the_release_key_they_name() {
    let Some(tools) = tools() else {
        eprintln!("skipped: gpg, node, sh or one of {UTILITIES:?} is not on PATH");
        return;
    };
    let source =
        |path: &str| std::fs::read_to_string(workspace().join(path)).expect("read an installer");
    let install_sh = source("install.sh");
    let postinstall = source("npm/scripts/postinstall.js");

    let key_in = |text: &str| {
        let begin = "-----BEGIN PGP PUBLIC KEY BLOCK-----";
        let end = "-----END PGP PUBLIC KEY BLOCK-----";
        let start = text.find(begin).expect("an armored key");
        let stop = text.find(end).expect("the key's end") + end.len();
        text[start..stop].to_owned()
    };
    let name_in = |text: &str| {
        let marker = "SIGNING_KEY_FINGERPRINT";
        let rest = &text[text.find(marker).expect("a named fingerprint") + marker.len()..];
        let digits: String = rest
            .chars()
            .filter(char::is_ascii_hexdigit)
            .take(40)
            .collect();
        assert_eq!(digits.len(), 40, "a fingerprint of forty hex digits");
        digits
    };

    let key = key_in(&install_sh);
    assert_eq!(
        key,
        key_in(&postinstall),
        "the installers carry different keys"
    );
    let named = name_in(&install_sh);
    assert_eq!(
        named,
        name_in(&postinstall),
        "the installers name different fingerprints"
    );

    let home = scratch_root().join("embedded-key");
    fresh(&home);
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700))
        .expect("narrow the keyring");
    let key_path = home.join("release.asc");
    std::fs::write(&key_path, &key).expect("write the embedded key");
    let listing = gpg(
        tools,
        &home,
        &[
            "--with-colons",
            "--import-options",
            "show-only",
            "--import",
            key_path.to_str().expect("utf-8 path"),
        ],
    );
    let actual = String::from_utf8(listing)
        .expect("utf-8 listing")
        .lines()
        .find_map(|line| line.strip_prefix("fpr:"))
        .and_then(|rest| rest.split(':').nth(8))
        .expect("a fingerprint line")
        .to_owned();
    assert_eq!(actual, named, "the embedded key is not the one named");
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
    ]) {
        assert_installed(installer, &outcome, &release.binary);
        assert_eq!(
            outcome.left_in_own_keyring,
            Vec::<PathBuf>::new(),
            "{installer:?} wrote to the person's own keyring"
        );
    }
}

/// What each argument to the script does, observed as the requests it makes. The version is the one
/// input from outside that reaches a URL, so what matters is which URLs: a good version downloads
/// from that tag under the fixed repository with no lookup of the newest, and anything else makes
/// no request at all.
#[test]
fn the_script_installs_the_version_it_is_given_and_refuses_what_is_not_one() {
    let Some((tools, release)) = setup() else {
        return;
    };
    let files: [(&str, &[u8]); 3] = [
        ("bravebot-linux-arm64", &release.binary),
        ("bravebot-linux-arm64.sha256", &release.checksum),
        ("bravebot-linux-arm64.sha256.asc", &release.signature),
    ];
    let base = "https://github.com/brave/bravebot/releases/download/v1.2.3";
    for argument in ["1.2.3", "v1.2.3"] {
        let install = Install::new(tools, release, "pinned").with_arguments(&[argument]);
        let outcome = install.run(Installer::Script, &files);
        assert_installed(Installer::Script, &outcome, &release.binary);
        assert_eq!(
            outcome.requests,
            [
                format!("{base}/bravebot-linux-arm64"),
                format!("{base}/bravebot-linux-arm64.sha256"),
                format!("{base}/bravebot-linux-arm64.sha256.asc"),
            ],
            "{argument:?} did not download from the tag it names"
        );
        assert!(outcome.output.contains("v1.2.3"), "{}", outcome.output);
    }

    let newest = Install::new(tools, release, "newest").run(Installer::Script, &files);
    assert_installed(Installer::Script, &newest, &release.binary);
    assert!(
        newest.requests[0].ends_with("/releases/latest")
            && newest.requests[1].contains("/releases/download/v0.0.0/"),
        "no argument stopped asking for the newest release: {:?}",
        newest.requests
    );

    let refused: [&[&str]; 11] = [
        &["../x"],
        &["1.2"],
        &[""],
        &["1.2.3.4"],
        &["1.2.3."],
        &[".1.2.3"],
        &["1..3"],
        &["1.2.3/../../x"],
        &["vv1.2.3"],
        &["V1.2.3"],
        &["1.2.3", "1.2.4"],
    ];
    for arguments in refused {
        let install = Install::new(tools, release, "refused").with_arguments(arguments);
        let outcome = install.run(Installer::Script, &files);
        assert_refused(Installer::Script, &outcome, "version");
        assert_eq!(
            outcome.requests,
            Vec::<String>::new(),
            "{arguments:?} reached the network"
        );
    }
}

/// A version with no release stops at the asset, with the version named.
#[test]
fn a_version_with_no_release_is_refused_naming_the_version() {
    let Some((tools, release)) = setup() else {
        return;
    };
    let install = Install::new(tools, release, "missing").with_arguments(&["9.9.9"]);
    let outcome = install.run(Installer::Script, &[]);
    assert_refused(Installer::Script, &outcome, "v9.9.9");
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
    ]) {
        assert_refused(installer, &outcome, "signature verification failed");
    }
}

/// A checksum signed by any key but the release key installs nothing. The signer here is a key
/// made beside the release key, as somebody replacing the release would need one of their own.
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
    ]) {
        let because = match installer {
            Installer::Script => "checksum signature",
            Installer::Npm => "bravebot-linux-arm64.sha256.asc",
        };
        assert_refused(installer, &outcome, because);
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

/// A macOS binary as the stand-in tools read one: signed by `team` and notarized, or neither.
fn mac_binary(team: Option<&str>, notarized: bool) -> Vec<u8> {
    let mut bytes = b"#!/bin/sh\necho released\n".to_vec();
    if let Some(team) = team {
        bytes.extend(format!("# signed-by: {team}\n").into_bytes());
    }
    if notarized {
        bytes.extend(b"# notarized\n");
    }
    bytes
}

/// The checksum file for `bytes`, made without the keyring directory the Linux release is made in.
fn checksum_of(tools: &Tools, bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    use std::process::Stdio;
    let sha256sum = tools
        .utilities
        .iter()
        .find(|(name, _)| *name == "sha256sum")
        .map(|(_, path)| path)
        .expect("sha256sum was resolved");
    let mut child = Command::new(sha256sum)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("run sha256sum");
    child
        .stdin
        .take()
        .expect("sha256sum stdin")
        .write_all(bytes)
        .expect("feed sha256sum");
    let output = child.wait_with_output().expect("wait for sha256sum");
    let digest = String::from_utf8(output.stdout).expect("utf-8 digest");
    format!("{}\n", digest.split_whitespace().next().expect("a digest")).into_bytes()
}

const DARWIN_ASSET: &str = "bravebot-darwin-arm64";
const WINDOWS_ASSET: &str = "bravebot-windows-arm64.exe";
const BRAVE_TEAM: &str = "KL8N8XSYF4";

/// Run both installers on macOS against `binary`, whose checksum matches it, so that the signature
/// is the only thing that can be wrong with it.
fn install_on_darwin(
    tools: &Tools,
    release: &Release,
    name: &str,
    binary: &[u8],
    customize: impl FnOnce(Install) -> Install,
) -> Vec<(Installer, Outcome)> {
    let install = customize(Install::new(tools, release, name).on(Platform::Darwin));
    let checksum = checksum_of(tools, binary);
    install.each(&[
        (DARWIN_ASSET, binary),
        ("bravebot-darwin-arm64.sha256", &checksum),
    ])
}

fn install_on_windows(
    tools: &Tools,
    release: &Release,
    name: &str,
    customize: impl FnOnce(Install) -> Install,
) -> Vec<(Installer, Outcome)> {
    let install = customize(Install::new(tools, release, name).on(Platform::Windows));
    let checksum = checksum_of(tools, &release.binary);
    install.each(&[
        (WINDOWS_ASSET, &release.binary),
        ("bravebot-windows-arm64.exe.sha256", &checksum),
    ])
}

/// Nothing is published beside a macOS or Windows checksum, so an installer that asked for a
/// signature there would fail every install. A binary that is signed by Brave and notarized
/// installs, and leaves nothing beside the executable.
#[test]
fn a_darwin_binary_signed_by_brave_and_notarized_installs() {
    let Some((tools, release)) = setup() else {
        return;
    };
    let binary = mac_binary(Some(BRAVE_TEAM), true);
    for (installer, outcome) in install_on_darwin(tools, release, "mac-good", &binary, |i| i) {
        assert_installed(installer, &outcome, &binary);
    }
}

/// A valid Developer ID signature from another team is what anybody with an Apple developer
/// account can make, and `codesign --verify` without a requirement accepts it.
#[test]
fn a_darwin_binary_signed_by_another_team_installs_nothing() {
    let Some((tools, release)) = setup() else {
        return;
    };
    let binary = mac_binary(Some("ABCDE12345"), true);
    for (installer, outcome) in install_on_darwin(tools, release, "mac-team", &binary, |i| i) {
        assert_refused(installer, &outcome, "not signed by brave software");
    }
}

#[test]
fn an_unsigned_darwin_binary_installs_nothing() {
    let Some((tools, release)) = setup() else {
        return;
    };
    let binary = mac_binary(None, false);
    for (installer, outcome) in install_on_darwin(tools, release, "mac-unsigned", &binary, |i| i) {
        assert_refused(installer, &outcome, "not signed by brave software");
    }
}

/// Signed by Brave and not notarized is what a binary signed with the right certificate but never
/// submitted to Apple looks like. It would run from a terminal and be refused on a double-click.
#[test]
fn a_darwin_binary_that_is_not_notarized_installs_nothing() {
    let Some((tools, release)) = setup() else {
        return;
    };
    let binary = mac_binary(Some(BRAVE_TEAM), false);
    for (installer, outcome) in install_on_darwin(tools, release, "mac-plain", &binary, |i| i) {
        assert_refused(installer, &outcome, "gatekeeper rejected");
    }
}

/// A Mac without the tool cannot check the signature, and installing anyway is what a check that
/// treats the tool's absence as a pass would do.
#[test]
fn a_darwin_install_without_codesign_installs_nothing() {
    let Some((tools, release)) = setup() else {
        return;
    };
    let binary = mac_binary(Some(BRAVE_TEAM), true);
    for (installer, outcome) in install_on_darwin(tools, release, "mac-no-codesign", &binary, |i| {
        i.without("codesign")
    }) {
        assert_refused(installer, &outcome, "codesign");
    }
}

#[test]
fn a_windows_binary_signed_by_brave_installs() {
    let Some((tools, release)) = setup() else {
        return;
    };
    for (installer, outcome) in install_on_windows(tools, release, "win-good", |i| i) {
        assert_installed(installer, &outcome, &release.binary);
    }
}

/// A valid Authenticode signature by anybody is what a status of `Valid` alone would accept.
#[test]
fn a_windows_binary_signed_by_someone_else_installs_nothing() {
    let Some((tools, release)) = setup() else {
        return;
    };
    for (installer, outcome) in install_on_windows(tools, release, "win-signer", |i| {
        i.authenticode("Valid", "Someone Else Ltd")
    }) {
        assert_refused(installer, &outcome, "not signed by brave software, inc.");
    }
}

#[test]
fn a_windows_binary_whose_signature_is_not_valid_installs_nothing() {
    let Some((tools, release)) = setup() else {
        return;
    };
    for status in ["NotSigned", "HashMismatch", "NotTrusted"] {
        for (installer, outcome) in install_on_windows(tools, release, "win-status", |i| {
            i.authenticode(status, "Brave Software, Inc.")
        }) {
            assert_refused(installer, &outcome, &status.to_lowercase());
        }
    }
}

#[test]
fn a_windows_install_without_powershell_installs_nothing() {
    let Some((tools, release)) = setup() else {
        return;
    };
    for (installer, outcome) in install_on_windows(tools, release, "win-no-powershell", |i| {
        i.without("powershell.exe")
    }) {
        assert_refused(installer, &outcome, "powershell.exe could not be run");
    }
}

/// Linux binaries carry no code signature, so neither tool is asked and none need be installed.
#[test]
fn a_linux_install_runs_neither_codesign_nor_powershell() {
    let Some((tools, release)) = setup() else {
        return;
    };
    let install = Install::new(tools, release, "linux-no-tools")
        .without("codesign")
        .without("spctl")
        .without("powershell.exe");
    for (installer, outcome) in install.each(&[
        ("bravebot-linux-arm64", &release.binary),
        ("bravebot-linux-arm64.sha256", &release.checksum),
        ("bravebot-linux-arm64.sha256.asc", &release.signature),
    ]) {
        assert_installed(installer, &outcome, &release.binary);
    }
}
