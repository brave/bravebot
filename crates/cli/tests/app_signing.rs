//! What `make app-sign`, `app-notarize`, `app-dmg-notarize` and `app-release-signed` hand to
//! `ui/scripts/sign-mac.mjs` and `spctl`, in what order, and which verdicts they accept.
//!
//! [RELEASE-23] is the clause. The targets are shell in a makefile, so running them against a copy
//! of the Makefile in a scratch directory is the only way a Rust test can hold them to it. The
//! tools are made here: `node` records the call to the script, and `spctl`, `ditto` and `hdiutil`
//! record their arguments, with `spctl` printing a verdict as the real one does, `ditto` copying a
//! tree and `hdiutil` writing a file where the real ones write an archive. `node` and `spctl` fail,
//! as the real ones do, when a path they are handed does not exist from the directory they run
//! in, so a target that changes directory between two commands is caught. What the
//! script asks of `codesign` and `notarytool` is held by `ui/scripts/sign-mac.test.mjs`, which runs
//! the libraries it calls. What the real tools accept, and whether Apple notarises the result, was
//! not checked: that needs a Developer ID identity and the notary service.
//!
//! Unix only, because the stand-in tools are shell scripts and the targets are run by `make`.
//! Skipped where `make` is not on `PATH`, and says so.
//!
//! [RELEASE-23]: ../../../docs/specs/releases.md

#![cfg(unix)]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};

const IDENTITY: &str = "Developer ID Application: Test";
const APPLE: &str = "APPLE=signer@example.com:abcd-efgh:ABCDE12345";

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

/// Accepts the path as the real `spctl` does, on standard error, from the source `FAKE_SOURCE`
/// names or else from a notarised Developer ID signature.
const VERDICT: &str = r#"printf '%s: accepted\nsource=%s\n' "$last" "${FAKE_SOURCE:-Notarized Developer ID}" >&2
"#;

/// Stands in for `node ui/scripts/sign-mac.mjs <command> <path>...`. The call is recorded with the
/// directory it ran in and the settings it was given. It fails, as `node` and the script do,
/// when the script or a path is not there from that directory.
const FAKE_NODE: &str = r#"#!/bin/sh
printf 'node\t%s\t%s\t%s\t' "$PWD" "SIGN_IDENTITY=$SIGN_IDENTITY" "APPLE=$APPLE_ID:$APPLE_ID_PASSWORD:$APPLE_TEAM_ID" >> "$FAKE_LOG"
for argument in "$@"; do printf '%s\t' "$argument" >> "$FAKE_LOG"; done
printf '\n' >> "$FAKE_LOG"
script="$1"; command="$2"; shift 2
[ -e "$script" ] || { echo "node: cannot find $script" >&2; exit 3; }
for path in "$@"; do
  [ -e "$path" ] || { echo "$command: no $path" >&2; exit 3; }
done
"#;

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

fn tools_available() -> bool {
    let found = Command::new("make").arg("--version").output().is_ok();
    if !found {
        eprintln!("skipped: make is not on PATH");
    }
    found
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
                format!("{FAKE_SPCTL}{RECORD}{LAST_ARGUMENT_EXISTS}{VERDICT}"),
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
            command.env(name, value);
        }
        command.output().expect("run make")
    }

    /// `make` with the identity and the notary credentials a release job sets in its environment.
    fn run(&self, target: &str) -> Output {
        self.run_with(target, &[])
    }

    /// `run`, with `more` in the environment as well.
    fn run_with(&self, target: &str, more: &[(&str, &str)]) -> Output {
        let mut variables = vec![
            ("SIGN_IDENTITY", IDENTITY),
            ("APPLE_ID", "signer@example.com"),
            ("APPLE_ID_PASSWORD", "abcd-efgh"),
            ("APPLE_TEAM_ID", "ABCDE12345"),
        ];
        variables.extend_from_slice(more);
        self.make(target, &variables)
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

/// Each app is notarised with the credentials the job gave, then assessed as an executable.
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
    assert_eq!(notarising[0][3], APPLE, "{calls:?}");
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
/// submitted with the credentials, then is assessed as an installer image.
///
/// The regression it rejects: assessing an image with the executable test, which refuses a disk
/// image, giving the script only the credentials, which leaves the image unsigned for the service to
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
        [format!("SIGN_IDENTITY={IDENTITY}"), APPLE.to_owned()],
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
            [format!("SIGN_IDENTITY={IDENTITY}"), APPLE.to_owned()],
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

/// An app or an image that Gatekeeper accepts from any source but a notarised Developer ID
/// signature stops the target with the path named.
///
/// The regression it rejects: trusting the exit status of `spctl`, which is 0 for an Apple or App
/// Store signature as well.
#[test]
fn an_item_accepted_from_another_source_stops_the_target() {
    if !tools_available() {
        return;
    }
    for (target, path) in [
        (
            "app-notarize",
            "ui/dist/Brave Bot-darwin-arm64/Brave Bot.app",
        ),
        ("app-dmg-notarize", "dist/bravebot-app-darwin-arm64.dmg"),
    ] {
        let tree = Tree::new(&format!("{target}-source"));
        tree.add_images();
        let output = tree.run_with(target, &[("FAKE_SOURCE", "Apple System")]);
        assert!(!output.status.success(), "{target} passed");
        assert!(
            stderr(&output).contains(&format!(
                "{path} is not accepted as a notarised Developer ID item"
            )),
            "{target}: {}",
            stderr(&output)
        );
    }
}
