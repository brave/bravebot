//! What `make app-sign`, `app-notarize`, `app-dmg-notarize`, `app-dmg-sha256` and
//! `app-release-signed` ask of `codesign`, `notarytool`, `stapler` and `spctl`, and in what order.
//!
//! [RELEASE-23] is the clause. The targets are shell in a makefile, so running them against a copy
//! of the Makefile in a scratch directory is the only way a Rust test can hold them to it. The
//! tools are made here: each records its arguments, `notarytool` answers with the verdict the test
//! sets, `stapler` appends to the image it is given as the real one rewrites it, and `ditto` and
//! `hdiutil` write a file where the real ones write an archive. What the real tools accept, and
//! whether Apple notarises the result, was not checked: that needs a Developer ID identity and
//! the notary service.
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
const APP_ENTITLEMENTS: &str = "ui/build/entitlements.mac.plist";
const HELPER_ENTITLEMENTS: &str = "ui/build/entitlements.mac.inherit.plist";

/// Records its arguments, tab separated, one call to a line.
const RECORD: &str = r#"printf '%s\t' "$TOOL" >> "$FAKE_LOG"
for argument in "$@"; do printf '%s\t' "$argument" >> "$FAKE_LOG"; done
printf '\n' >> "$FAKE_LOG"
"#;

const FAKE_CODESIGN: &str = "#!/bin/sh\nTOOL=codesign\n";
const FAKE_SPCTL: &str = "#!/bin/sh\nTOOL=spctl\n";

/// `notarytool submit` answers with the status the test names and exits 0 whatever it is, as the
/// real one does for a rejected submission. `stapler staple` changes the bytes of an image.
const FAKE_XCRUN: &str = r#"#!/bin/sh
TOOL=xcrun
case "$1 $2" in
  "notarytool submit")
    printf '  id: 1\n  status: %s\n' "${FAKE_VERDICT:-Accepted}" ;;
  "stapler staple")
    if [ -d "$3" ]; then : > "$3/Contents/ticket"; else printf 'stapled\n' >> "$3"; fi ;;
esac
"#;

/// Makes an archive of the first form and copies a tree for the second.
const FAKE_DITTO: &str = r#"#!/bin/sh
TOOL=ditto
if [ "$1" = "-c" ]; then
  for argument in "$@"; do last="$argument"; done
  printf 'zip\n' > "$last"
else
  cp -R "$1" "$2"
fi
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

/// A copy of the Makefile beside the two bundles an `app-bundles` run leaves, each with the
/// executables, a framework holding a helper, a dylib and a helper app, and the stand-in tools.
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
        for electron in ["arm64", "x64"] {
            let bundle = root.join(format!("ui/dist/Brave Bot-darwin-{electron}"));
            let app = bundle.join("Brave Bot.app/Contents");
            for file in [
                "Resources/bravebot-rpc",
                "Resources/bravebot-ui-files",
                "Frameworks/Electron Framework.framework/Versions/A/Helpers/crashpad_handler",
                "Frameworks/Squirrel.framework/Versions/A/Resources/ShipIt",
                "Frameworks/Squirrel.framework/Versions/A/Resources/Info.plist",
                "Frameworks/Electron Framework.framework/Versions/A/Libraries/libffmpeg.dylib",
                "Frameworks/Brave Bot Helper.app/Contents/MacOS/Brave Bot Helper",
                "MacOS/Brave Bot",
            ] {
                let path = app.join(file);
                std::fs::create_dir_all(path.parent().expect("parent")).expect("create dir");
                std::fs::write(&path, "binary\n").expect("write file");
                // Electron's framework Resources hold ShipIt, which is executable, beside
                // property lists, which are not and are not signed on their own.
                let mode = if file.ends_with("Info.plist") {
                    0o644
                } else {
                    0o755
                };
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))
                    .expect("set mode");
            }
            for licence in ["LICENSE", "LICENSES.chromium.html"] {
                std::fs::write(bundle.join(licence), "licence\n").expect("write licence");
            }
        }
        for (name, body) in [
            ("codesign", FAKE_CODESIGN),
            ("spctl", FAKE_SPCTL),
            ("xcrun", FAKE_XCRUN),
            ("ditto", FAKE_DITTO),
            ("hdiutil", FAKE_HDIUTIL),
        ] {
            let path = root.join("bin").join(name);
            std::fs::write(&path, format!("{body}{RECORD}")).expect("write tool");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("make tool executable");
        }
        Self { root }
    }

    fn make(&self, target: &str, verdict: &str, variables: &[(&str, &str)]) -> Output {
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
            .env("FAKE_LOG", self.log())
            .env("FAKE_VERDICT", verdict);
        for (name, value) in variables {
            command.arg(format!("{name}={value}"));
        }
        command.output().expect("run make")
    }

    /// `make` with the identity and the profile a release job sets.
    fn run(&self, target: &str) -> Output {
        self.make(
            target,
            "Accepted",
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

    /// The signing calls, with the path each signs.
    fn signatures(&self) -> Vec<Vec<String>> {
        self.calls()
            .into_iter()
            .filter(|call| call[0] == "codesign" && call.iter().any(|a| a == "--sign"))
            .collect()
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

/// The path a call acts on: its last argument.
fn target(call: &[String]) -> &str {
    call.last().expect("a call has arguments")
}

fn position(calls: &[Vec<String>], wanted: impl Fn(&[String]) -> bool, what: &str) -> usize {
    calls
        .iter()
        .position(|call| wanted(call))
        .unwrap_or_else(|| panic!("no call for {what} in {calls:?}"))
}

fn signed_at(path_part: &'static str) -> impl Fn(&[String]) -> bool {
    move |call: &[String]| call.iter().any(|a| a == "--sign") && target(call).ends_with(path_part)
}

/// For each bundle, the two executables are signed before the helper inside a framework and the
/// executable in a framework's `Resources` (Squirrel's `ShipIt`), those and the dylib before the
/// framework, the framework before the helper app, and the helper app before the outer app, which
/// is last. A property list beside `ShipIt` is not signed on its own.
///
/// The regression it rejects: signing the outer app first or `--deep`, which seals a nested item
/// before it is signed, so the outer signature is invalidated when the nested one is made and
/// notarisation rejects the app.
#[test]
fn the_bundle_is_signed_from_the_inside_out() {
    if !tools_available() {
        return;
    }
    let tree = Tree::new("order");
    let output = tree.run("app-sign");
    assert!(output.status.success(), "{}", stderr(&output));
    let calls = tree.signatures();
    for electron in ["arm64", "x64"] {
        let app = format!("Brave Bot-darwin-{electron}/Brave Bot.app");
        let inside = |part: &'static str| {
            let wanted = signed_at(part);
            calls
                .iter()
                .position(|call| wanted(call) && target(call).contains(app.as_str()))
                .unwrap_or_else(|| panic!("{part} of {app} not signed: {calls:?}"))
        };
        let rpc = inside("Resources/bravebot-rpc");
        let files = inside("Resources/bravebot-ui-files");
        let crashpad = inside("Helpers/crashpad_handler");
        let shipit = inside("Resources/ShipIt");
        let dylib = inside("libffmpeg.dylib");
        let framework = inside("Electron Framework.framework");
        let helper_app = inside("Brave Bot Helper.app");
        let outer = calls
            .iter()
            .position(|call| target(call).ends_with(app.as_str()))
            .expect("outer app signed");
        assert!(rpc < crashpad && files < crashpad, "{calls:?}");
        assert!(crashpad < framework && dylib < framework, "{calls:?}");
        assert!(
            rpc < shipit && files < shipit && shipit < framework,
            "{calls:?}"
        );
        assert!(
            !calls
                .iter()
                .any(|call| target(call).ends_with("Info.plist")),
            "a property list was signed on its own: {calls:?}"
        );
        assert!(framework < helper_app, "{calls:?}");
        assert!(helper_app < outer, "{calls:?}");
        assert_eq!(
            calls
                .iter()
                .filter(|call| target(call).ends_with(&app))
                .count(),
            1,
            "the outer app is signed more than once: {calls:?}"
        );
        let last_for_app = calls
            .iter()
            .rposition(|call| target(call).contains(&app))
            .expect("signed");
        assert_eq!(
            outer, last_for_app,
            "something is signed after the app: {calls:?}"
        );
    }
}

/// Every signature asks for the hardened runtime and a secure timestamp, uses the identity it was
/// given, and none passes `--deep`.
///
/// The regression it rejects: dropping `--options runtime` or `--timestamp`, which notarisation
/// refuses, and `--deep`, which applies one set of entitlements to everything beneath the app.
#[test]
fn every_signature_is_hardened_timestamped_and_not_deep() {
    if !tools_available() {
        return;
    }
    let tree = Tree::new("flags");
    let output = tree.run("app-sign");
    assert!(output.status.success(), "{}", stderr(&output));
    let calls = tree.signatures();
    assert_eq!(calls.len(), 18, "two bundles of nine signatures: {calls:?}");
    for call in &calls {
        let joined = call.join(" ");
        assert!(joined.contains("--options runtime"), "{joined}");
        assert!(call.iter().any(|a| a == "--timestamp"), "{joined}");
        assert!(joined.contains(&format!("--sign {IDENTITY}")), "{joined}");
        assert!(!joined.contains("--deep"), "{joined}");
    }
}

/// The outer app is signed with the app's entitlements, the helper app with the helpers', and the
/// two Rust executables, frameworks, dylibs and the framework's helper with none.
///
/// The regression it rejects: swapping the two files, giving every item the app's, or giving the
/// agent and the file helper, which are Rust executables that run no JIT-compiled code, the
/// `allow-jit` entitlement meant for the Electron processes, which widens what a compromised agent
/// process may do.
#[test]
fn each_item_is_signed_with_the_entitlements_it_is_meant_to_carry() {
    if !tools_available() {
        return;
    }
    let tree = Tree::new("entitlements");
    let output = tree.run("app-sign");
    assert!(output.status.success(), "{}", stderr(&output));
    let entitlements = |call: &[String]| {
        call.iter()
            .position(|a| a == "--entitlements")
            .map(|index| call[index + 1].clone())
    };
    for call in tree.signatures() {
        let path = target(&call);
        let wanted = if path.ends_with("Brave Bot.app") {
            Some(APP_ENTITLEMENTS)
        } else if path.ends_with("Brave Bot Helper.app") {
            Some(HELPER_ENTITLEMENTS)
        } else {
            None
        };
        assert_eq!(entitlements(&call).as_deref(), wanted, "{path}");
    }
}

/// With no identity nothing is signed, and a missing second bundle stops the target before the
/// first is signed.
///
/// The regression it rejects: signing with an empty identity, which `codesign` reads as ad hoc and
/// so produces a signature notarisation refuses, and signing one architecture before finding the
/// other missing.
#[test]
fn signing_stops_before_a_tool_runs_when_it_cannot_complete() {
    if !tools_available() {
        return;
    }
    let tree = Tree::new("refuse");
    let output = tree.make("app-sign", "Accepted", &[]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("SIGN_IDENTITY"),
        "{}",
        stderr(&output)
    );
    assert!(tree.calls().is_empty(), "{:?}", tree.calls());

    std::fs::remove_dir_all(tree.root.join("ui/dist/Brave Bot-darwin-x64")).expect("remove");
    let output = tree.run("app-sign");
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("make app-bundles"),
        "{}",
        stderr(&output)
    );
    assert!(tree.calls().is_empty(), "{:?}", tree.calls());
}

/// An app is submitted, stapled, then assessed as an executable, and each of those happens for
/// both architectures.
///
/// The regression it rejects: stapling before the submission is accepted, skipping the assessment,
/// or assessing with the image's test (`-t open`), which accepts what `-t exec` refuses.
#[test]
fn an_accepted_app_is_stapled_then_assessed_as_an_executable() {
    if !tools_available() {
        return;
    }
    let tree = Tree::new("app-notarize");
    let output = tree.run("app-notarize");
    assert!(output.status.success(), "{}", stderr(&output));
    let calls = tree.calls();
    for electron in ["arm64", "x64"] {
        let app = format!("Brave Bot-darwin-{electron}/Brave Bot.app");
        let staple = position(
            &calls,
            |call| call[1] == "stapler" && target(call).ends_with(&app),
            "stapling the app",
        );
        let assess = position(
            &calls,
            |call| call[0] == "spctl" && target(call).ends_with(&app),
            "assessing the app",
        );
        assert!(staple < assess, "{calls:?}");
        let assessed = calls[assess].join(" ");
        assert!(assessed.contains("-a -t exec"), "{assessed}");
    }
    let submissions: Vec<_> = calls
        .iter()
        .filter(|call| call[1] == "notarytool")
        .collect();
    assert_eq!(submissions.len(), 2, "{calls:?}");
    for submission in submissions {
        let joined = submission.join(" ");
        assert!(joined.contains("--wait"), "{joined}");
        assert!(
            joined.contains(&format!("--keychain-profile {PROFILE}")),
            "{joined}"
        );
    }
}

/// A submission the service did not accept stops the target before anything is stapled or
/// assessed, though `notarytool` exited 0.
///
/// The regression it rejects: trusting the exit status of `notarytool submit --wait`, which is 0
/// for an `Invalid` verdict, so a rejected app is stapled, which fails late and reads as a
/// problem with stapling.
#[test]
fn a_rejected_submission_stops_before_stapling() {
    if !tools_available() {
        return;
    }
    for (target_name, subject) in [("app-notarize", "app"), ("app-dmg-notarize", "image")] {
        let tree = Tree::new(&format!("rejected-{subject}"));
        if subject == "image" {
            std::fs::write(
                tree.root.join("dist/bravebot-app-darwin-arm64.dmg"),
                "image\n",
            )
            .expect("write");
            std::fs::write(
                tree.root.join("dist/bravebot-app-darwin-amd64.dmg"),
                "image\n",
            )
            .expect("write");
        }
        let output = tree.make(
            target_name,
            "Invalid",
            &[("SIGN_IDENTITY", IDENTITY), ("NOTARY_PROFILE", PROFILE)],
        );
        assert!(!output.status.success(), "{subject} was accepted");
        assert!(
            stderr(&output).contains("not accepted"),
            "{}",
            stderr(&output)
        );
        let calls = tree.calls();
        assert!(
            calls
                .iter()
                .all(|call| call[1] != "stapler" && call[0] != "spctl"),
            "{subject}: {calls:?}"
        );
    }
}

/// A target that notarises stops before it runs a tool when the profile, the identity it signs an
/// image with, a bundle or an image is missing, and says what to set or run.
///
/// The regression it rejects: submitting the first architecture and finding the second missing
/// afterwards, which leaves one app or image notarised and the release half done, and running
/// `notarytool` with an empty profile, which fails late with an error about the keychain.
#[test]
fn notarising_stops_before_a_tool_runs_when_it_cannot_complete() {
    if !tools_available() {
        return;
    }
    let tree = Tree::new("notarise-refuse");
    for arch in ["arm64", "amd64"] {
        std::fs::write(
            tree.root
                .join(format!("dist/bravebot-app-darwin-{arch}.dmg")),
            "image\n",
        )
        .expect("write");
    }
    for (target_name, variables, says) in [
        (
            "app-notarize",
            vec![("SIGN_IDENTITY", IDENTITY)],
            "NOTARY_PROFILE",
        ),
        (
            "app-dmg-notarize",
            vec![("SIGN_IDENTITY", IDENTITY)],
            "NOTARY_PROFILE",
        ),
        (
            "app-dmg-notarize",
            vec![("NOTARY_PROFILE", PROFILE)],
            "SIGN_IDENTITY",
        ),
    ] {
        let output = tree.make(target_name, "Accepted", &variables);
        assert!(!output.status.success(), "{target_name} ran without {says}");
        assert!(stderr(&output).contains(says), "{}", stderr(&output));
        assert!(tree.calls().is_empty(), "{target_name}: {:?}", tree.calls());
    }

    std::fs::remove_dir_all(tree.root.join("ui/dist/Brave Bot-darwin-x64")).expect("remove");
    let output = tree.run("app-notarize");
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("make app-sign"),
        "{}",
        stderr(&output)
    );
    assert!(tree.calls().is_empty(), "{:?}", tree.calls());

    std::fs::remove_file(tree.root.join("dist/bravebot-app-darwin-amd64.dmg")).expect("remove");
    let output = tree.run("app-dmg-notarize");
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("make app-dmg"),
        "{}",
        stderr(&output)
    );
    assert!(tree.calls().is_empty(), "{:?}", tree.calls());
}

/// An image is signed, submitted, stapled and then assessed as an installer image with the
/// primary-signature context, in that order, for both architectures.
///
/// The regression it rejects: notarising an image that is not signed, which the service refuses,
/// and assessing it with the executable test, which refuses a disk image.
#[test]
fn an_accepted_image_is_signed_stapled_and_assessed_as_an_installer() {
    if !tools_available() {
        return;
    }
    let tree = Tree::new("dmg-notarize");
    for arch in ["arm64", "amd64"] {
        std::fs::write(
            tree.root
                .join(format!("dist/bravebot-app-darwin-{arch}.dmg")),
            "image\n",
        )
        .expect("write");
    }
    let output = tree.run("app-dmg-notarize");
    assert!(output.status.success(), "{}", stderr(&output));
    let calls = tree.calls();
    for arch in ["arm64", "amd64"] {
        let dmg = format!("dist/bravebot-app-darwin-{arch}.dmg");
        let sign = position(
            &calls,
            |call| call[0] == "codesign" && target(call) == dmg,
            "signing the image",
        );
        let submit = position(
            &calls,
            |call| {
                call[1] == "notarytool" && call.iter().any(|a| a.ends_with(&format!("{arch}.dmg")))
            },
            "submitting the image",
        );
        let staple = position(
            &calls,
            |call| call[1] == "stapler" && target(call) == dmg,
            "stapling the image",
        );
        let assess = position(
            &calls,
            |call| call[0] == "spctl" && target(call) == dmg,
            "assessing the image",
        );
        assert!(
            sign < submit && submit < staple && staple < assess,
            "{calls:?}"
        );
        let assessed = calls[assess].join(" ");
        assert!(
            assessed.contains("-a -t open --context context:primary-signature"),
            "{assessed}"
        );
        assert!(
            calls[sign].iter().any(|a| a == "--timestamp"),
            "{:?}",
            calls[sign]
        );
    }
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
    let output = tree.make("checksums", "Accepted", &[]);
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
