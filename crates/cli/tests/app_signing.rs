//! What `make app-sign`, `app-notarize`, `app-dmg-notarize`, `app-dmg-sha256` and
//! `app-release-signed` hand to `ui/scripts/sign-mac.mjs` and `spctl`, and in what order.
//!
//! [RELEASE-23] is the clause. The targets are shell in a makefile, so running them against a copy
//! of the Makefile in a scratch directory is the only way a Rust test can hold them to it. The
//! tools are made here: `node` records the call to the script and, for an image, appends to it as
//! stapling rewrites it, and `spctl`, `ditto` and `hdiutil` record their arguments, with `ditto`
//! copying a tree and `hdiutil` writing a file where the real ones write an archive. `node` and
//! `spctl` fail, as the real ones do, when a path they are handed does not exist from the directory
//! they run in, so a target that changes directory between two commands is caught. What the
//! script asks of `codesign` and `notarytool` is held by `ui/scripts/sign-mac.test.mjs`, which runs
//! the libraries it calls. What the real tools accept, and whether Apple notarises the result, was
//! not checked: that needs a Developer ID identity and the notary service.
//!
//! Unix only, because the stand-in tools are shell scripts and the targets are run by `make`.
//! Skipped where `make` or `shasum` is not on `PATH`, and says so.
//!
//! [RELEASE-23]: ../../../docs/specs/releases.md

#![cfg(unix)]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};

const IDENTITY: &str = "Developer ID Application: Test";
const PROFILE: &str = "test-profile";

/// Records its arguments, tab separated, one call to a line.
const RECORD: &str = r#"printf '%s\t' "$TOOL" >> "$FAKE_LOG"
for argument in "$@"; do printf '%s\t' "$argument" >> "$FAKE_LOG"; done
printf '\n' >> "$FAKE_LOG"
"#;

const FAKE_SPCTL: &str = "#!/bin/sh\nTOOL=spctl\n";

/// Fails as `spctl` does when the path it is asked to assess is not there.
const LAST_ARGUMENT_EXISTS: &str = r#"for last; do :; done
[ -e "$last" ] || { echo "$TOOL: $last: No such file or directory" >&2; exit 3; }
"#;

/// Stands in for `node ui/scripts/sign-mac.mjs <command> <path>...`. The call is recorded with the
/// directory it ran in and the two settings it was given. It fails, as `node` and the script do,
/// when the script or a path is not there from that directory. For `image` each path is appended
/// to, as notarising and stapling rewrite the image.
const FAKE_NODE: &str = r#"#!/bin/sh
printf 'node\t%s\t%s\t%s\t' "$PWD" "SIGN_IDENTITY=$SIGN_IDENTITY" "NOTARY_PROFILE=$NOTARY_PROFILE" >> "$FAKE_LOG"
for argument in "$@"; do printf '%s\t' "$argument" >> "$FAKE_LOG"; done
printf '\n' >> "$FAKE_LOG"
script="$1"; command="$2"; shift 2
[ -e "$script" ] || { echo "node: cannot find $script" >&2; exit 3; }
for path in "$@"; do
  [ -e "$path" ] || { echo "$command: no $path" >&2; exit 3; }
done
if [ "$command" = image ]; then
  for image in "$@"; do printf 'stapled\n' >> "$image"; done
fi
"#;

/// Copies a tree.
const FAKE_DITTO: &str = r#"#!/bin/sh
TOOL=ditto
cp -R "$1" "$2"
"#;

const FAKE_HDIUTIL: &str = r#"#!/bin/sh
TOOL=hdiutil
for argument in "$@"; do last="$argument"; done
printf 'image %s\n' "${last##*/}" > "$last"
"#;

fn workspace() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path
}

fn available(program: &str) -> bool {
    let found = Command::new(program).arg("--version").output().is_ok();
    if !found {
        eprintln!("skipped: {program} is not on PATH");
    }
    found
}

fn tools_available() -> bool {
    available("make")
        && Command::new("shasum")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
}

/// A copy of the Makefile beside the two bundles an `app-bundles` run leaves, and the stand-in
/// tools.
struct Tree {
    root: PathBuf,
}

impl Tree {
    fn new(name: &str) -> Self {
        let root = workspace()
            .join("target/test-scratch/app-signing")
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("dist")).expect("create scratch");
        std::fs::create_dir_all(root.join("bin")).expect("create bin");
        std::fs::copy(workspace().join("Makefile"), root.join("Makefile")).expect("copy Makefile");
        std::fs::create_dir_all(root.join("ui/scripts")).expect("create scripts");
        std::fs::write(root.join("ui/scripts/sign-mac.mjs"), "").expect("write script");
        for electron in ["arm64", "x64"] {
            let bundle = root.join(format!("ui/dist/Brave Bot-darwin-{electron}"));
            let executable = bundle.join("Brave Bot.app/Contents/MacOS/Brave Bot");
            std::fs::create_dir_all(executable.parent().expect("parent")).expect("create dir");
            std::fs::write(&executable, "binary\n").expect("write executable");
            for licence in ["LICENSE", "LICENSES.chromium.html"] {
                std::fs::write(bundle.join(licence), "licence\n").expect("write licence");
            }
        }
        for (name, body) in [
            (
                "spctl",
                format!("{FAKE_SPCTL}{RECORD}{LAST_ARGUMENT_EXISTS}"),
            ),
            ("ditto", format!("{FAKE_DITTO}{RECORD}")),
            ("hdiutil", format!("{FAKE_HDIUTIL}{RECORD}")),
            ("node", FAKE_NODE.to_owned()),
        ] {
            let path = root.join("bin").join(name);
            std::fs::write(&path, body).expect("write tool");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("make tool executable");
        }
        Self { root }
    }

    fn make(&self, target: &str, variables: &[(&str, &str)]) -> Output {
        let path = format!(
            "{}:{}",
            self.root.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut command = Command::new("make");
        command
            .arg("-C")
            .arg(&self.root)
            .arg(target)
            .env("PATH", path)
            .env("FAKE_LOG", self.log());
        for (name, value) in variables {
            command.arg(format!("{name}={value}"));
        }
        command.output().expect("run make")
    }

    /// `make` with the identity and the profile a release job sets.
    fn run(&self, target: &str) -> Output {
        self.make(
            target,
            &[("SIGN_IDENTITY", IDENTITY), ("NOTARY_PROFILE", PROFILE)],
        )
    }

    fn log(&self) -> PathBuf {
        self.root.join("tools.log")
    }

    /// Each call the tools recorded: the tool, then its arguments.
    fn calls(&self) -> Vec<Vec<String>> {
        std::fs::read_to_string(self.log())
            .unwrap_or_default()
            .lines()
            .map(|line| {
                line.split('\t')
                    .filter(|part| !part.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .collect()
    }

    /// Writes an image where `app-dmg` leaves one.
    fn add_images(&self) {
        for arch in ["arm64", "amd64"] {
            std::fs::write(
                self.root
                    .join(format!("dist/bravebot-app-darwin-{arch}.dmg")),
                "image\n",
            )
            .expect("write");
        }
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

/// The calls to the script with `command`: the directory, the two settings, the script, then the
/// command and the paths.
fn script_calls<'a>(calls: &'a [Vec<String>], command: &str) -> Vec<&'a Vec<String>> {
    calls
        .iter()
        .filter(|call| call[0] == "node" && call[5] == command)
        .collect()
}

/// The position of the first call that `wanted` accepts.
fn position(calls: &[Vec<String>], wanted: impl Fn(&[String]) -> bool, what: &str) -> usize {
    calls
        .iter()
        .position(|call| wanted(call))
        .unwrap_or_else(|| panic!("no call for {what} in {calls:?}"))
}

/// Both bundles go to the script in one call, as paths from the repository root, with the identity
/// the job gave.
///
/// The regression it rejects: one call per architecture, which signs the first before the second
/// is found missing, paths that are not from the directory the target runs the script in, and not
/// passing the identity on.
#[test]
fn the_apps_are_signed_by_the_script_with_the_identity_the_job_gives() {
    if !tools_available() {
        return;
    }
    let tree = Tree::new("sign");
    let output = tree.run("app-sign");
    assert!(output.status.success(), "{}", stderr(&output));
    let calls = tree.calls();
    let signing = script_calls(&calls, "sign");
    assert_eq!(signing.len(), 1, "{calls:?}");
    let call = signing[0];
    assert_eq!(
        std::fs::canonicalize(&call[1]).expect("the directory it ran in"),
        std::fs::canonicalize(&tree.root).expect("the scratch root"),
        "{call:?}"
    );
    assert_eq!(call[2], format!("SIGN_IDENTITY={IDENTITY}"), "{call:?}");
    assert_eq!(call[4], "ui/scripts/sign-mac.mjs", "{call:?}");
    assert_eq!(
        call[6..],
        [
            "ui/dist/Brave Bot-darwin-arm64/Brave Bot.app",
            "ui/dist/Brave Bot-darwin-x64/Brave Bot.app"
        ],
        "{call:?}"
    );
}

/// Each app is notarised with the profile the job gave, then assessed as an executable.
///
/// The regression it rejects: skipping the assessment, which is the only check that Gatekeeper
/// accepts the stapled app, assessing before the script has stapled, assessing with the image's
/// test (`-t open`), which accepts what `-t exec` refuses, and assessing from a directory the
/// script's `cd` left behind, which names a bundle that is not there and fails the release.
#[test]
fn a_notarised_app_is_assessed_as_an_executable() {
    if !tools_available() {
        return;
    }
    let tree = Tree::new("app-notarize");
    let output = tree.run("app-notarize");
    assert!(output.status.success(), "{}", stderr(&output));
    let calls = tree.calls();
    let notarising = script_calls(&calls, "notarize");
    assert_eq!(notarising.len(), 1, "{calls:?}");
    assert_eq!(
        notarising[0][3],
        format!("NOTARY_PROFILE={PROFILE}"),
        "{calls:?}"
    );
    let notarised = position(&calls, |call| call[0] == "node", "notarising");
    for electron in ["arm64", "x64"] {
        let app = format!("ui/dist/Brave Bot-darwin-{electron}/Brave Bot.app");
        let assess = position(
            &calls,
            |call| call[0] == "spctl" && call.last() == Some(&app),
            "assessing the app",
        );
        assert!(notarised < assess, "{calls:?}");
        assert_eq!(calls[assess][1..4], ["-a", "-t", "exec"], "{calls:?}");
    }
}

/// Each image goes to the script with both settings, because it is signed with the identity and
/// submitted with the profile, then is assessed as an installer image.
///
/// The regression it rejects: assessing an image with the executable test, which refuses a disk
/// image, giving the script only the profile, which leaves the image unsigned for the service to
/// refuse, and assessing from a directory the script's `cd` left behind, which names an image that
/// is not there and fails the release.
#[test]
fn a_notarised_image_is_assessed_as_an_installer() {
    if !tools_available() {
        return;
    }
    let tree = Tree::new("dmg-notarize");
    tree.add_images();
    let output = tree.run("app-dmg-notarize");
    assert!(output.status.success(), "{}", stderr(&output));
    let calls = tree.calls();
    let images = script_calls(&calls, "image");
    assert_eq!(images.len(), 1, "{calls:?}");
    assert_eq!(
        images[0][2..4],
        [
            format!("SIGN_IDENTITY={IDENTITY}"),
            format!("NOTARY_PROFILE={PROFILE}")
        ],
        "{calls:?}"
    );
    assert_eq!(
        images[0][6..],
        [
            "dist/bravebot-app-darwin-arm64.dmg",
            "dist/bravebot-app-darwin-amd64.dmg"
        ],
        "{calls:?}"
    );
    let notarised = position(&calls, |call| call[0] == "node", "notarising");
    for arch in ["arm64", "amd64"] {
        let dmg = format!("dist/bravebot-app-darwin-{arch}.dmg");
        let assess = position(
            &calls,
            |call| call[0] == "spctl" && call.last() == Some(&dmg),
            "assessing the image",
        );
        assert!(notarised < assess, "{calls:?}");
        assert_eq!(
            calls[assess][1..5],
            ["-a", "-t", "open", "--context"],
            "{calls:?}"
        );
        assert_eq!(calls[assess][5], "context:primary-signature", "{calls:?}");
    }
}

/// The sub-targets of `app-release-signed` run in the order that puts a stapled ticket inside each
/// image: every app is signed before any is notarised, every app is notarised before an image is
/// built, and every image is built before one is signed and notarised.
///
/// The regression it rejects: swapping `app-dmg` above `app-notarize`, which builds images holding
/// apps with no ticket, and `app-notarize` above `app-sign`, which submits an unsigned app. The
/// tests that run one sub-target alone pass either way.
#[test]
fn the_sub_targets_run_in_the_order_that_puts_the_ticket_in_the_image() {
    if !tools_available() {
        return;
    }
    let tree = Tree::new("release-order");
    let output = tree.run("app-release-signed");
    assert!(output.status.success(), "{}", stderr(&output));
    let calls = tree.calls();
    for call in calls.iter().filter(|call| call[0] == "node") {
        assert_eq!(
            call[2..4],
            [
                format!("SIGN_IDENTITY={IDENTITY}"),
                format!("NOTARY_PROFILE={PROFILE}")
            ],
            "a sub-target did not get the settings: {call:?}"
        );
    }
    let at = |command: &str| {
        position(
            &calls,
            |call| call[0] == "node" && call[5] == command,
            command,
        )
    };
    let first_image = position(&calls, |call| call[0] == "hdiutil", "building an image");
    let last_image = calls
        .iter()
        .rposition(|call| call[0] == "hdiutil")
        .expect("an image was built");
    assert!(
        at("sign") < at("notarize"),
        "an app was submitted before it was signed: {calls:?}"
    );
    assert!(
        at("notarize") < first_image,
        "an image was built before an app was notarised: {calls:?}"
    );
    assert!(
        last_image < at("image"),
        "an image was signed or notarised before all were built: {calls:?}"
    );
}

/// After the whole sequence each image has a `.sha256` holding the digest of the bytes the image
/// has then, after stapling changed them, and nothing but the digest.
///
/// The regression it rejects: hashing the image before it is stapled, which publishes a digest
/// no download matches, and writing the digest with a filename beside it, which the installers
/// refuse. It also holds the target to hashing the two images and nothing else in `dist/`.
#[test]
fn the_checksum_of_each_image_covers_its_final_bytes() {
    if !tools_available() {
        return;
    }
    let tree = Tree::new("sha256");
    std::fs::write(tree.root.join("dist/bravebot-rpc-darwin-arm64"), "helper\n").expect("write");
    let output = tree.run("app-release-signed");
    assert!(output.status.success(), "{}", stderr(&output));
    for arch in ["arm64", "amd64"] {
        let dmg = tree
            .root
            .join(format!("dist/bravebot-app-darwin-{arch}.dmg"));
        let bytes = std::fs::read_to_string(&dmg).expect("read image");
        assert!(
            bytes.ends_with("stapled\n"),
            "the image was not stapled: {bytes}"
        );
        let expected = Command::new("shasum")
            .args(["-a", "256"])
            .arg(&dmg)
            .output()
            .expect("run shasum");
        let expected = String::from_utf8_lossy(&expected.stdout)
            .split_whitespace()
            .next()
            .expect("a digest")
            .to_owned();
        let written =
            std::fs::read_to_string(format!("{}.sha256", dmg.display())).expect("read digest");
        assert_eq!(written, format!("{expected}\n"));
    }
    let mut names: Vec<String> = std::fs::read_dir(tree.root.join("dist"))
        .expect("read dist")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(".sha256"))
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "bravebot-app-darwin-amd64.dmg.sha256".to_owned(),
            "bravebot-app-darwin-arm64.dmg.sha256".to_owned(),
        ]
    );
}

/// The checksum target writes nothing when an image is missing.
///
/// The regression it rejects: writing the digest of the image that exists and leaving the other
/// without one, which publishes half of the assets of a release.
#[test]
fn a_missing_image_gets_no_checksum_at_all() {
    if !tools_available() {
        return;
    }
    let tree = Tree::new("sha256-missing");
    std::fs::write(
        tree.root.join("dist/bravebot-app-darwin-arm64.dmg"),
        "image\n",
    )
    .expect("write");
    let output = tree.run("app-dmg-sha256");
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("make app-dmg"),
        "{}",
        stderr(&output)
    );
    assert!(
        !tree
            .root
            .join("dist/bravebot-app-darwin-arm64.dmg.sha256")
            .exists()
    );
}

/// `make checksums` leaves a disk image out of `SHA256SUMS` and gives it no `.sha256`.
///
/// The regression it rejects: the `bravebot-*` loop matching `bravebot-app-darwin-*.dmg`, which
/// puts the digest of an image in the list before it is stapled, and `app-dmg-sha256` never
/// rewrites that list.
#[test]
fn the_cli_checksums_leave_a_disk_image_out() {
    if !tools_available() {
        return;
    }
    let tree = Tree::new("checksums-skip-image");
    for name in ["bravebot-darwin-arm64", "bravebot-app-darwin-arm64.dmg"] {
        std::fs::write(tree.root.join("dist").join(name), "bytes\n").expect("write");
    }
    let output = tree.make("checksums", &[]);
    assert!(output.status.success(), "{}", stderr(&output));
    let sums = std::fs::read_to_string(tree.root.join("dist/SHA256SUMS")).expect("read sums");
    assert!(sums.contains("bravebot-darwin-arm64"), "{sums}");
    assert!(!sums.contains(".dmg"), "{sums}");
    assert!(
        !tree
            .root
            .join("dist/bravebot-app-darwin-arm64.dmg.sha256")
            .exists()
    );
}
