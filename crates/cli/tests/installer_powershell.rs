//! What `install.ps1` does with a release, observed by running it under `pwsh` against a release
//! served from a directory. It is the Windows counterpart of `install.sh`, and what it checks before
//! it writes an executable is [RELEASE-9], [RELEASE-13] and [RELEASE-15]; what it writes down is
//! [UPDATE-10] and [STATE-2].
//!
//! The script is run as it is published, with no hook for a test in it. The commands that reach the
//! network or the operating system are `Invoke-RestMethod`, `Invoke-WebRequest`,
//! `Get-AuthenticodeSignature` and, for the person's stored `Path`, `Get-Item` and
//! `Set-ItemProperty`, and a function of the same name defined where the script is called
//! from is found before the command, so the stand-ins live in a harness that runs the script and
//! the script is not told. `Invoke-WebRequest` answers from a directory by the last segment of the
//! URL, as `install.sh`'s `curl` does here. `Get-AuthenticodeSignature` answers for the file it is
//! given, from marker lines in it, as `codesign` does in `installer_signature.rs`: a stand-in that
//! answered from the environment would say the same thing about whichever file the script asked
//! about. What the real command says about Brave's binaries has not been observed on Windows, which
//! `docs/specs/releases.md` records under RELEASE-15. The stored `Path` is a file named for the
//! registry value, read and written through the same calls the script makes of the registry, and
//! what the script leaves in it and in the session's `PATH` is what [RELEASE-20] holds.
//!
//! Skipped where `pwsh` or a SHA-256 tool is not on `PATH`, or on an architecture the script has no
//! asset for, and says so, except on the Linux job in CI, where a missing one fails the test. PowerShell on Linux and macOS has no `Get-AuthenticodeSignature`, so a
//! run with no stand-in is a run on a machine that cannot check the signature.
//!
//! [RELEASE-9]: ../../../docs/specs/releases.md
//! [RELEASE-13]: ../../../docs/specs/releases.md
//! [RELEASE-15]: ../../../docs/specs/releases.md
//! [RELEASE-20]: ../../../docs/specs/releases.md
//! [UPDATE-10]: ../../../docs/specs/updates.md
//! [STATE-2]: ../../../docs/specs/state-directory.md

#![cfg(unix)]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// The commands the script reaches the network and the operating system with, answered from files
/// here. Each takes exactly the parameters the script is expected to pass and no others, so a
/// script that asked for something else fails here rather than being answered.
const HARNESS: &str = r#"
$script = $args[0]
$arguments = @($args | Select-Object -Skip 1)

function Invoke-RestMethod {
    [CmdletBinding()] param([Parameter(Mandatory)][string]$Uri)
    Add-Content -LiteralPath $env:REQUESTS -Value $Uri
    if ($Uri -like '*/releases/latest') {
        $tag = if ($null -ne $env:LATEST_TAG) { $env:LATEST_TAG } else { 'v0.0.0' }
        return [pscustomobject]@{ tag_name = $tag }
    }
    throw "no stand-in for $Uri"
}

function Invoke-WebRequest {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$Uri, [Parameter(Mandatory)][string]$OutFile, [switch]$UseBasicParsing)
    Add-Content -LiteralPath $env:REQUESTS -Value $Uri
    $file = Join-Path $env:SERVED $Uri.Substring($Uri.LastIndexOf('/') + 1)
    if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw "HTTP 404 for $Uri" }
    Copy-Item -LiteralPath $file -Destination $OutFile
}

if ($env:SIGNATURE_TOOL -eq '1') {
    function Get-AuthenticodeSignature {
        [CmdletBinding()] param([Parameter(Mandatory)][string]$LiteralPath)
        Add-Content -LiteralPath $env:ASKED -Value $LiteralPath
        $status = 'NotSigned'
        $signer = $null
        foreach ($line in [System.IO.File]::ReadAllText($LiteralPath) -split "`n") {
            if ($line -match '^# authenticode: (.*)$') { $status = $Matches[1].TrimEnd("`r") }
            if ($line -match '^# signer: (.*)$') { $signer = $Matches[1].TrimEnd("`r") }
        }
        $certificate = $null
        if ($null -ne $signer) {
            $certificate = [pscustomobject]@{} |
                Add-Member -MemberType ScriptMethod -Name GetNameInfo -Value { param($type, $chain) $signer }.GetNewClosure() -PassThru
        }
        [pscustomobject]@{ Status = $status; SignerCertificate = $certificate }
    }
}

# The user's stored PATH, kept in a file named for the registry value. Reading it without
# `DoNotExpandEnvironmentNames` expands `%NAME%` as the real registry does, so a script that asks
# for the expanded text and writes it back changes what the person had.
function Get-Item {
    [CmdletBinding()] param([Parameter(Mandatory)][string]$LiteralPath)
    if ($LiteralPath -ne 'HKCU:\Environment') { throw "no stand-in for $LiteralPath" }
    [pscustomobject]@{} | Add-Member -MemberType ScriptMethod -Name GetValue -PassThru -Value {
        param($name, $default, $options)
        Add-Content -LiteralPath $env:REGISTRY_LOG -Value "read $name $options"
        $file = Join-Path $env:REGISTRY $name
        if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { return $default }
        $stored = [System.IO.File]::ReadAllText($file)
        if ("$options" -eq 'DoNotExpandEnvironmentNames') { return $stored }
        [Environment]::ExpandEnvironmentVariables($stored)
    }
}

function Set-ItemProperty {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$LiteralPath, [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string]$Value, [Parameter(Mandatory)][string]$Type)
    if ($LiteralPath -ne 'HKCU:\Environment') { throw "no stand-in for $LiteralPath" }
    if ($env:REGISTRY_WRITE -eq 'denied') { throw 'Requested registry access is not allowed.' }
    Add-Content -LiteralPath $env:REGISTRY_LOG -Value "write $Name"
    [System.IO.File]::WriteAllText((Join-Path $env:REGISTRY $Name), $Value)
    [System.IO.File]::WriteAllText((Join-Path $env:REGISTRY "$Name.kind"), $Type)
}

# `pwsh` puts its own directory on the PATH it starts, so what the script is compared against is what
# the session had when the script began and not what the test gave the process.
[System.IO.File]::WriteAllText("$($env:SESSION_PATH).start", [string]$env:PATH)
$failed = $false
try { & $script @arguments } catch { [Console]::Error.WriteLine($_.Exception.Message); $failed = $true }
# What a person's session is left with, which is what `bravebot` is looked up in next.
[System.IO.File]::WriteAllText($env:SESSION_PATH, [string]$env:PATH)
if ($failed) { exit 1 }
"#;

/// The signer Brave's Windows binaries carry.
const BRAVE: &str = "Brave Software, Inc.";

/// The installed file's name, which the signature check reads a file by.
const BINARY: &str = "bravebot.exe";

/// The programs these tests run, found on the `PATH` the test runner was given.
struct Tools {
    pwsh: PathBuf,
    /// A program that prints a SHA-256 digest of the file named last, and the arguments before it.
    hasher: (PathBuf, &'static [&'static str]),
    /// The asset name's architecture, which the script takes from the machine it runs on.
    arch: &'static str,
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
            let arch = match std::env::consts::ARCH {
                "x86_64" => "amd64",
                "aarch64" => "arm64",
                _ => return None,
            };
            let hasher = on_path("sha256sum")
                .map(|path| (path, &[][..]))
                .or_else(|| on_path("shasum").map(|path| (path, &["-a", "256"][..])))?;
            Some(Tools {
                pwsh: on_path("pwsh")?,
                hasher,
                arch,
            })
        })
        .as_ref()
}

fn setup() -> Option<&'static Tools> {
    let tools = tools();
    if tools.is_none() {
        // The Linux job in CI has all of these, so there a missing one is a runner that changed,
        // and skipping would pass every clause that cites these tests having checked nothing.
        assert!(
            !(cfg!(target_os = "linux") && std::env::var_os("GITHUB_ACTIONS").is_some()),
            "pwsh, sha256sum or shasum is not on PATH on a CI runner that has them"
        );
        eprintln!(
            "skipped: pwsh, sha256sum or shasum is not on PATH, or this architecture has no Windows asset"
        );
    }
    tools
}

fn workspace() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path
}

/// Under this workspace's build directory rather than the system temporary one, which is shared
/// between users and where a name this predictable is somebody else's to create first.
fn scratch_root() -> PathBuf {
    workspace().join("target/test-scratch/powershell")
}

fn fresh(path: &Path) {
    let _ = std::fs::remove_dir_all(path);
    std::fs::create_dir_all(path).expect("create scratch");
}

/// The digest of `bytes` as a checksum file holds it: sixty-four hex digits and a newline.
fn checksum(tools: &Tools, bytes: &[u8]) -> Vec<u8> {
    let directory = scratch_root().join("digest-input");
    std::fs::create_dir_all(&directory).expect("create scratch");
    let path = directory.join(format!(
        "{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(&path, bytes).expect("write digest input");
    let (program, arguments) = &tools.hasher;
    let output = Command::new(program)
        .args(*arguments)
        .arg(&path)
        .output()
        .expect("run the hasher");
    let _ = std::fs::remove_file(&path);
    let digest = String::from_utf8(output.stdout).expect("utf-8 digest");
    format!("{}\n", digest.split_whitespace().next().expect("a digest")).into_bytes()
}

/// A Windows binary as the stand-in for `Get-AuthenticodeSignature` reads one: a status and a
/// signer, or neither for a binary that carries no signature.
fn binary(status: Option<&str>, signer: Option<&str>) -> Vec<u8> {
    let mut bytes = b"MZ a stand-in for a Windows executable\n".to_vec();
    if let Some(status) = status {
        bytes.extend(format!("# authenticode: {status}\n").into_bytes());
    }
    if let Some(signer) = signer {
        bytes.extend(format!("# signer: {signer}\n").into_bytes());
    }
    bytes
}

/// A binary signed by Brave. `body` makes two of them differ, so a test can tell which release was
/// installed by what is on disk.
fn brave(body: &str) -> Vec<u8> {
    let mut bytes = binary(Some("Valid"), Some(BRAVE));
    bytes.extend(format!("# release: {body}\n").into_bytes());
    bytes
}

/// How the person's profile directory is named to the script.
#[derive(Clone, Copy)]
enum Profile {
    /// `HOME`, which is what a shell on Unix, and a Windows shell somebody configured, sets.
    Home,
    /// `USERPROFILE` alone, which is all that stock Windows sets.
    Windows,
    /// Neither, or both empty, as on a machine whose profile cannot be found.
    Neither,
}

/// One install, run in directories nobody else is using and removed afterwards.
struct Install<'a> {
    tools: &'a Tools,
    root: PathBuf,
    arguments: Vec<String>,
    environment: Vec<(String, String)>,
    /// Whether the stand-in for `Get-AuthenticodeSignature` exists.
    signature_tool: bool,
    profile: Profile,
    /// What `INSTALL_DIR` is set to, if it is set.
    install_dir: Option<PathBuf>,
    /// Whether `PATH` names the directory the executable goes to.
    on_path: bool,
    /// What the user's stored `Path` holds when the run starts, or `None` for a profile with no
    /// such value.
    stored_path: Option<String>,
    /// Whether the stand-in for writing the registry refuses.
    registry_write_denied: bool,
    /// Whether this run made the directories and removes them when it is done.
    owned: bool,
}

/// What an install left behind.
struct Outcome {
    succeeded: bool,
    output: String,
    /// Anything the script made in the temporary directory it was given.
    left_in_temporary: Vec<String>,
    /// Every URL the script asked for, in order.
    requests: Vec<String>,
    /// Every file the script asked about the signature of, in order.
    asked: Vec<String>,
    /// Whatever the script left in the directory it was started in.
    in_working_directory: Vec<PathBuf>,
    /// Every read and write the script made of the user's stored environment, in order, each as
    /// `read <name> <options>` or `write <name>`.
    registry: Vec<String>,
    /// The user's stored `Path` after the run, and the kind it was last written as.
    stored_path_after: Option<String>,
    stored_kind: Option<String>,
    /// The `PATH` the person's session has when the script starts, and is left with.
    session_path_before: String,
    session_path: String,
}

impl<'a> Install<'a> {
    fn new(tools: &'a Tools, name: &str) -> Self {
        let root = scratch_root().join(name);
        fresh(&root);
        for directory in [
            "served", "tmp", "home", "bin", "cwd", "log", "profile", "registry",
        ] {
            std::fs::create_dir_all(root.join(directory)).expect("create a scratch directory");
        }
        Self::at(tools, &root, true)
    }

    /// Another run in directories an earlier one made, for a test of what a second run does with
    /// what the first left. Only the run that made them removes them.
    fn at(tools: &'a Tools, root: &Path, owned: bool) -> Self {
        Self {
            tools,
            root: root.to_path_buf(),
            install_dir: Some(root.join("bin")),
            arguments: Vec::new(),
            environment: Vec::new(),
            signature_tool: true,
            profile: Profile::Home,
            on_path: true,
            stored_path: None,
            registry_write_denied: false,
            owned,
        }
    }

    fn with_arguments(mut self, arguments: &[&str]) -> Self {
        self.arguments = arguments
            .iter()
            .map(|argument| (*argument).to_owned())
            .collect();
        self
    }

    fn with_environment(mut self, name: &str, value: &str) -> Self {
        self.environment.push((name.to_owned(), value.to_owned()));
        self
    }

    fn without_signature_tool(mut self) -> Self {
        self.signature_tool = false;
        self
    }

    fn with_profile(mut self, profile: Profile) -> Self {
        self.profile = profile;
        self
    }

    fn installing_to(mut self, directory: Option<PathBuf>) -> Self {
        self.install_dir = directory;
        self
    }

    fn off_path(mut self) -> Self {
        self.on_path = false;
        self
    }

    fn with_stored_path(mut self, stored: &str) -> Self {
        self.stored_path = Some(stored.to_owned());
        self
    }

    fn with_registry_write_denied(mut self) -> Self {
        self.registry_write_denied = true;
        self
    }

    /// The `PATH` the session starts with, which is the directory the executable goes to when
    /// `on_path` and somewhere that is not otherwise.
    fn session_path_at_start(&self) -> PathBuf {
        if self.on_path {
            self.bin()
        } else {
            self.root.join("elsewhere")
        }
    }

    fn bin(&self) -> PathBuf {
        self.root.join("bin")
    }

    /// Where the profile directory is, whichever variable names it.
    fn profile_dir(&self) -> PathBuf {
        self.root.join("profile")
    }

    /// The record the script writes, which is where the profile's `.bravebot` is.
    fn record(&self) -> PathBuf {
        self.profile_dir()
            .join(".bravebot")
            .join("installed-by-powershell")
    }

    /// The directory the harness and the stand-ins are written to, apart from what is served.
    fn harness(&self) -> PathBuf {
        let path = self.root.join("log").join("harness.ps1");
        std::fs::write(&path, HARNESS).expect("write the harness");
        path
    }

    /// Serve `files`, each named as its URL ends, and run the script once.
    fn run(&self, files: &[(&str, &[u8])]) -> Outcome {
        let served = self.root.join("served");
        for (name, bytes) in files {
            std::fs::write(served.join(name), bytes).expect("serve a file");
        }
        let temporary = self.root.join("tmp");
        let requests = self.root.join("log").join("requests");
        let asked = self.root.join("log").join("asked");
        let working = self.root.join("cwd");
        let path = self.session_path_at_start();
        let registry = self.root.join("registry");
        let registry_log = self.root.join("log").join("registry");
        let session_path = self.root.join("log").join("session-path");
        if let Some(stored) = &self.stored_path {
            std::fs::write(registry.join("Path"), stored).expect("store a user path");
        }

        let mut command = Command::new(&self.tools.pwsh);
        command
            .env_clear()
            .current_dir(&working)
            .args(["-NoProfile", "-NonInteractive", "-File"])
            .arg(self.harness())
            .arg(workspace().join("install.ps1"))
            .args(&self.arguments)
            .env("PATH", &path)
            .env("TMPDIR", &temporary)
            .env("SERVED", &served)
            .env("REQUESTS", &requests)
            .env("ASKED", &asked)
            .env("REGISTRY", &registry)
            .env("REGISTRY_LOG", &registry_log)
            .env("SESSION_PATH", &session_path)
            .env(
                "REGISTRY_WRITE",
                if self.registry_write_denied {
                    "denied"
                } else {
                    "allowed"
                },
            )
            .env(
                "SIGNATURE_TOOL",
                if self.signature_tool { "1" } else { "0" },
            )
            .env("LOCALAPPDATA", self.root.join("local-app-data"))
            // PowerShell keeps its own files under whatever it takes for the home directory, so
            // it is given one of its own whichever way the script's profile is named.
            .env("DOTNET_CLI_HOME", self.root.join("home"))
            .env("XDG_DATA_HOME", self.root.join("home"))
            .env("XDG_CONFIG_HOME", self.root.join("home"))
            .env("XDG_CACHE_HOME", self.root.join("home"))
            .env("NO_COLOR", "1")
            .env("POWERSHELL_TELEMETRY_OPTOUT", "1")
            .env("POWERSHELL_UPDATECHECK", "Off")
            .env("DOTNET_CLI_TELEMETRY_OPTOUT", "1");
        match self.profile {
            Profile::Home => {
                command.env("HOME", self.profile_dir());
            }
            // The script reads HOME first, so a HOME that names something would win. A shell on
            // Windows that sets none is what an empty one stands for.
            Profile::Windows => {
                command
                    .env("HOME", "")
                    .env("USERPROFILE", self.profile_dir());
            }
            Profile::Neither => {
                command.env("HOME", "").env("USERPROFILE", "");
            }
        }
        if let Some(directory) = &self.install_dir {
            command.env("INSTALL_DIR", directory);
        }
        for (name, value) in &self.environment {
            command.env(name, value);
        }
        let output = command.output().expect("run pwsh");

        let lines = |path: &Path| -> Vec<String> {
            std::fs::read_to_string(path)
                .unwrap_or_default()
                .lines()
                .map(str::to_owned)
                .collect()
        };
        Outcome {
            succeeded: output.status.success(),
            output: format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
            left_in_temporary: entries(&temporary)
                .into_iter()
                .filter_map(|path| {
                    path.file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                })
                .filter(|name| name.starts_with("bravebot-install-"))
                .collect(),
            requests: lines(&requests),
            asked: lines(&asked),
            in_working_directory: entries(&working),
            registry: lines(&registry_log),
            stored_path_after: std::fs::read_to_string(registry.join("Path")).ok(),
            stored_kind: std::fs::read_to_string(registry.join("Path.kind")).ok(),
            session_path_before: std::fs::read_to_string(format!(
                "{}.start",
                session_path.display()
            ))
            .unwrap_or_default(),
            session_path: std::fs::read_to_string(&session_path).unwrap_or_default(),
        }
    }

    /// The asset the script asks for on this machine, and its checksum.
    fn asset(&self) -> String {
        format!("bravebot-windows-{}.exe", self.tools.arch)
    }

    /// A release made of `binary` and the checksum of it, as it is published.
    fn serving(&self, binary: &[u8]) -> Vec<(String, Vec<u8>)> {
        vec![
            (self.asset(), binary.to_vec()),
            (
                format!("{}.sha256", self.asset()),
                checksum(self.tools, binary),
            ),
        ]
    }

    /// Run against a release made of `binary` and its own checksum, so the signature is the only
    /// thing that can be wrong with it.
    fn run_release(&self, binary: &[u8]) -> Outcome {
        let files = self.serving(binary);
        let files: Vec<(&str, &[u8])> = files
            .iter()
            .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
            .collect();
        self.run(&files)
    }
}

impl Drop for Install<'_> {
    fn drop(&mut self) {
        if self.owned {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

fn entries(directory: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(directory)
        .map(|listing| {
            listing
                .map(|entry| entry.expect("directory entry").path())
                .collect()
        })
        .unwrap_or_default()
}

fn read(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

/// Refused, writing nothing anywhere, and for the reason given: a refusal the fixture caused by
/// accident is not evidence that the installer refuses what the test changed.
fn assert_refused(install: &Install, outcome: &Outcome, because: &str) {
    assert!(!outcome.succeeded, "installed: {}", outcome.output);
    assert_eq!(
        entries(&install.bin()),
        Vec::<PathBuf>::new(),
        "something was written where the executable goes"
    );
    assert!(
        outcome.output.to_lowercase().contains(because),
        "refused for a reason other than {because:?}: {}",
        outcome.output
    );
    assert_eq!(
        read(&install.record()),
        None,
        "a refused install recorded where it went"
    );
    assert_eq!(
        outcome.left_in_temporary,
        Vec::<String>::new(),
        "the download was left behind"
    );
    assert_eq!(
        outcome.in_working_directory,
        Vec::<PathBuf>::new(),
        "something was written where the script was started"
    );
    assert_eq!(
        outcome.registry,
        Vec::<String>::new(),
        "a refused install read or changed the stored environment"
    );
}

fn assert_installed(install: &Install, outcome: &Outcome, binary: &[u8]) {
    assert!(outcome.succeeded, "refused: {}", outcome.output);
    assert_eq!(
        read(&install.bin().join(BINARY)).as_deref(),
        Some(binary),
        "installed something other than the release"
    );
    assert_eq!(
        entries(&install.bin()),
        vec![install.bin().join(BINARY)],
        "something other than the executable was left where it goes"
    );
    assert_eq!(
        outcome.left_in_temporary,
        Vec::<String>::new(),
        "the download was left behind"
    );
}

/// The release as published installs, from the fixed repository, and what the signature was
/// asked about is the file that was installed. A file the real command reads by its extension is
/// named for one.
#[test]
fn a_windows_binary_signed_by_brave_installs() {
    let Some(tools) = setup() else {
        return;
    };
    let install = Install::new(tools, "signed");
    let release = brave("first");
    let outcome = install.run_release(&release);

    assert_installed(&install, &outcome, &release);
    assert!(
        outcome.output.contains("Installed bravebot v0.0.0 to "),
        "{}",
        outcome.output
    );
    let base = "https://github.com/brave/bravebot/releases/download/v0.0.0";
    assert_eq!(
        outcome.requests,
        [
            "https://api.github.com/repos/brave/bravebot/releases/latest".to_owned(),
            format!("{base}/{}", install.asset()),
            format!("{base}/{}.sha256", install.asset()),
        ],
        "the release was not looked up and fetched from the fixed repository"
    );
    assert_eq!(outcome.asked.len(), 1, "{:?}", outcome.asked);
    assert!(
        outcome.asked[0].ends_with(".exe"),
        "the signature was asked of a file the real command would not read as an executable: {:?}",
        outcome.asked
    );
}

/// What each argument does, observed as the requests it makes. The version is the one input from
/// outside that reaches a URL, so what matters is which URLs: a good version downloads from that
/// tag with no lookup of the newest, and anything else makes no request at all.
///
/// The refused ones are the shapes a looser check lets through: another script's digits, which a
/// pattern written with `\d` accepts, a trailing newline, which `$` accepts, an argument that is
/// given and empty, which is not the same as none, and a second argument.
#[test]
fn the_script_installs_the_version_it_is_given_and_refuses_what_is_not_one() {
    let Some(tools) = setup() else {
        return;
    };
    let base = "https://github.com/brave/bravebot/releases/download/v1.2.3";
    for argument in ["1.2.3", "v1.2.3"] {
        let install = Install::new(tools, "pinned").with_arguments(&[argument]);
        let release = brave("pinned");
        let outcome = install.run_release(&release);
        assert_installed(&install, &outcome, &release);
        assert_eq!(
            outcome.requests,
            [
                format!("{base}/{}", install.asset()),
                format!("{base}/{}.sha256", install.asset()),
            ],
            "{argument:?} did not download from the tag it names"
        );
        assert!(outcome.output.contains("v1.2.3"), "{}", outcome.output);
    }

    let refused: [&[&str]; 14] = [
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
        &["\u{ff11}.\u{ff12}.\u{ff13}"],
        &["1.2.3\n"],
        &["1.2.3\n../../x"],
        &["1.2.3", "1.2.4"],
    ];
    for arguments in refused {
        let install = Install::new(tools, "refused").with_arguments(arguments);
        let outcome = install.run_release(&brave("refused"));
        let because = if arguments.len() > 1 {
            "positional parameter"
        } else {
            "version"
        };
        assert_refused(&install, &outcome, because);
        assert_eq!(
            outcome.requests,
            Vec::<String>::new(),
            "{arguments:?} reached the network"
        );
    }
}

/// A version with no release stops at the asset, with the version named, and writes nothing.
#[test]
fn a_version_with_no_release_is_refused_naming_the_version() {
    let Some(tools) = setup() else {
        return;
    };
    let install = Install::new(tools, "missing").with_arguments(&["9.9.9"]);
    let outcome = install.run(&[]);
    assert_refused(&install, &outcome, "v9.9.9");
}

/// RELEASE-13: the newest release's tag is composed into a URL, and it arrives in a response, so it
/// is held to the shape of a published one before it is. A tag with a path in it would make the
/// next request go somewhere under the repository's host that no release is, and one that is not
/// `v` and three numbers is not a release this program knows how to name.
#[test]
fn a_newest_release_tag_that_is_not_a_version_installs_nothing() {
    let Some(tools) = setup() else {
        return;
    };
    for tag in [
        "v1.2",
        "v",
        "1.2.3",
        "vv1.2.3",
        "v1.2.3\n",
        "v1.2.3/../x",
        "../../evil/releases/download/v1.2.3",
        "v1.2.3-rc.1",
    ] {
        let install = Install::new(tools, "newest-tag").with_environment("LATEST_TAG", tag);
        let outcome = install.run_release(&brave("newest-tag"));
        assert_refused(
            &install,
            &outcome,
            "unable to resolve the latest release tag",
        );
        assert_eq!(
            outcome.requests,
            ["https://api.github.com/repos/brave/bravebot/releases/latest"],
            "{tag:?} was composed into a download URL"
        );
    }
}

/// A binary that is not the one the checksum is for installs nothing, whatever it says about its
/// signature. Replacing the binary and leaving the checksum is what the signature check alone
/// would pass, since the substitute is signed by Brave here.
#[test]
fn a_binary_that_does_not_match_its_checksum_installs_nothing() {
    let Some(tools) = setup() else {
        return;
    };
    let install = Install::new(tools, "mismatch");
    let published = brave("published");
    let substitute = brave("substitute");
    let outcome = install.run(&[
        (&install.asset(), &substitute),
        (
            &format!("{}.sha256", install.asset()),
            &checksum(tools, &published),
        ),
    ]);
    assert_refused(&install, &outcome, "checksum mismatch");
    assert_eq!(
        outcome.asked,
        Vec::<String>::new(),
        "a binary the checksum refused was still checked for its signature"
    );
}

/// A checksum file that is not a digest alone is a release published wrong. Each of these is what
/// a check that only looks for the digest somewhere in the file, or for a prefix of it, accepts.
#[test]
fn a_malformed_checksum_installs_nothing() {
    let Some(tools) = setup() else {
        return;
    };
    let release = brave("malformed");
    let digest = String::from_utf8(checksum(tools, &release)).expect("utf-8 digest");
    let digest = digest.trim();
    let asset = format!("bravebot-windows-{}.exe", tools.arch);
    let malformed = [
        format!("{digest}  {asset}\n"),
        digest[..63].to_owned(),
        format!("{digest}0"),
        format!("{}g", &digest[..63]),
        String::new(),
        format!("sha256:{digest}"),
    ];
    for contents in malformed {
        let install = Install::new(tools, "malformed");
        let outcome = install.run(&[
            (&asset, &release),
            (&format!("{asset}.sha256"), contents.as_bytes()),
        ]);
        assert_refused(&install, &outcome, "malformed checksum");
    }
}

/// The digest is compared as two digests and not as two spellings: `Get-FileHash` writes upper
/// case and a checksum file is lower case, and either case of the published one installs.
#[test]
fn a_checksum_in_either_case_installs() {
    let Some(tools) = setup() else {
        return;
    };
    let release = brave("cased");
    let digest = String::from_utf8(checksum(tools, &release)).expect("utf-8 digest");
    for contents in [
        digest.to_lowercase(),
        digest.to_uppercase(),
        digest.trim().to_owned(),
    ] {
        let install = Install::new(tools, "cased");
        let asset = install.asset();
        let outcome = install.run(&[
            (&asset, &release),
            (&format!("{asset}.sha256"), contents.as_bytes()),
        ]);
        assert_installed(&install, &outcome, &release);
    }
}

/// A valid Authenticode signature by anybody is what a status of `Valid` alone would accept, and
/// a signer that only resembles Brave's is what a comparison that ignores case or a trailing
/// character would accept.
#[test]
fn a_windows_binary_signed_by_someone_else_installs_nothing() {
    let Some(tools) = setup() else {
        return;
    };
    for signer in [
        "Someone Else Ltd",
        "Brave Software, Inc",
        "brave software, inc.",
        "Brave Software, Inc. ",
        "Not Brave Software, Inc.",
    ] {
        let install = Install::new(tools, "signer");
        let outcome = install.run_release(&binary(Some("Valid"), Some(signer)));
        assert_refused(&install, &outcome, "not signed by brave software, inc.");
    }
}

/// A binary that carries no signature has no signing certificate at all, which is its own shape.
#[test]
fn an_unsigned_windows_binary_installs_nothing() {
    let Some(tools) = setup() else {
        return;
    };
    let install = Install::new(tools, "unsigned");
    let outcome = install.run_release(&binary(None, None));
    assert_refused(&install, &outcome, "not signed by brave software, inc.");
    assert!(
        outcome.output.contains("signer none"),
        "the missing certificate was not said: {}",
        outcome.output
    );
}

/// Brave as the signer is not enough on its own: a signature that does not verify, or whose chain
/// is not trusted, is refused with the status named.
#[test]
fn a_windows_binary_whose_signature_is_not_valid_installs_nothing() {
    let Some(tools) = setup() else {
        return;
    };
    for status in [
        "NotSigned",
        "HashMismatch",
        "NotTrusted",
        "UnknownError",
        "valid",
    ] {
        let install = Install::new(tools, "status");
        let outcome = install.run_release(&binary(Some(status), Some(BRAVE)));
        assert_refused(
            &install,
            &outcome,
            &format!("authenticode reports {}", status.to_lowercase()),
        );
    }
}

/// A PowerShell that cannot check the signature, such as one on a machine that is not Windows,
/// installs nothing, and says which script is for the machine it is on. Skipping the check for
/// want of the tool is what an attacker arranges, and `Get-AuthenticodeSignature` ships with
/// Windows, so its absence is not a reason to trust the download.
#[test]
fn an_install_without_the_signature_tool_installs_nothing() {
    let Some(tools) = setup() else {
        return;
    };
    let install = Install::new(tools, "no-tool").without_signature_tool();
    let outcome = install.run_release(&brave("no-tool"));
    assert_refused(
        &install,
        &outcome,
        "get-authenticodesignature is not available",
    );
    assert!(
        outcome.output.contains("install.sh"),
        "the script for the other platforms was not named: {}",
        outcome.output
    );
    assert_eq!(
        outcome.requests,
        Vec::<String>::new(),
        "a download was made that could not be checked"
    );
}

/// What the script writes down, byte for byte: the path of the executable and a newline, in UTF-8
/// with no byte order mark. The program that reads it takes the first line as the path, so a mark
/// is part of the path and the record would name no file.
#[test]
fn the_record_is_the_path_of_the_executable_and_a_newline() {
    let Some(tools) = setup() else {
        return;
    };
    let install = Install::new(tools, "record");
    let release = brave("record");
    let outcome = install.run_release(&release);
    assert_installed(&install, &outcome, &release);

    let expected = format!("{}\n", install.bin().join(BINARY).display());
    assert_eq!(
        read(&install.record()),
        Some(expected.into_bytes()),
        "the record is not the executable's path and a newline"
    );
}

/// UPDATE-10: a later run with no directory named installs over the binary the record names, and
/// a directory named wins over it and is what the record names afterwards. The directory is
/// nested, not yet made, and spelled with a bracket and letters outside ASCII, so a record read back
/// in another encoding, a directory the script did not make, and a name taken for a pattern are each
/// a different place.
#[test]
fn a_later_run_installs_where_the_record_names_unless_a_directory_is_given() {
    let Some(tools) = setup() else {
        return;
    };
    let first = Install::new(tools, "again");
    let moved = first.root.join("bin-[\u{fc}]").join("n\u{e4}\u{ef}ve");
    let first = first.installing_to(Some(moved.clone()));
    let one = brave("one");
    let outcome = first.run_release(&one);
    assert!(outcome.succeeded, "{}", outcome.output);
    assert_eq!(read(&moved.join(BINARY)).as_deref(), Some(one.as_slice()));

    // No directory named: the record decides, so the default under LOCALAPPDATA stays unmade.
    let second = Install::at(tools, &first.root, false).installing_to(None);
    let two = brave("two");
    let outcome = second.run_release(&two);
    assert!(outcome.succeeded, "{}", outcome.output);
    assert_eq!(
        read(&moved.join(BINARY)).as_deref(),
        Some(two.as_slice()),
        "the update did not replace the binary the record names"
    );
    assert!(
        !first.root.join("local-app-data").exists(),
        "the update went to the default directory and left the recorded one"
    );
    assert_eq!(
        read(&first.record()),
        Some(format!("{}\n", moved.join(BINARY).display()).into_bytes())
    );

    // A directory named wins, and the record follows it.
    let elsewhere = first.root.join("elsewhere-bin");
    let third = Install::at(tools, &first.root, false).installing_to(Some(elsewhere.clone()));
    let three = brave("three");
    let outcome = third.run_release(&three);
    assert!(outcome.succeeded, "{}", outcome.output);
    assert_eq!(
        read(&elsewhere.join(BINARY)).as_deref(),
        Some(three.as_slice())
    );
    assert_eq!(
        read(&moved.join(BINARY)).as_deref(),
        Some(two.as_slice()),
        "a directory that was named still replaced the recorded one"
    );
    assert_eq!(
        read(&first.record()),
        Some(format!("{}\n", elsewhere.join(BINARY).display()).into_bytes())
    );
}

/// STATE-2: a profile directory that is not named is not a place to write. The script then joins
/// its state directory onto nothing and would make `.bravebot` in whatever directory it was started
/// in, where a record would decide where the next release lands. The binary is still installed.
/// `USERPROFILE` alone, which is all stock Windows sets, does name one.
#[test]
fn with_no_profile_directory_the_script_installs_and_records_nothing() {
    let Some(tools) = setup() else {
        return;
    };
    let install = Install::new(tools, "no-profile").with_profile(Profile::Neither);
    let release = brave("no-profile");
    let outcome = install.run_release(&release);
    assert_installed(&install, &outcome, &release);
    assert_eq!(
        outcome.in_working_directory,
        Vec::<PathBuf>::new(),
        "a state directory was made where the script was started"
    );
    assert_eq!(read(&install.record()), None);

    let install = Install::new(tools, "user-profile").with_profile(Profile::Windows);
    let outcome = install.run_release(&release);
    assert_installed(&install, &outcome, &release);
    assert_eq!(
        read(&install.record()),
        Some(format!("{}\n", install.bin().join(BINARY).display()).into_bytes()),
        "USERPROFILE did not name the state directory"
    );
}

/// STATE-2: the directory a person names in `BRAVEBOT_HOME` is the one the record goes in, as it is
/// the one the program reads `installed-by-powershell` from, and it answers before the profile
/// directory, so it holds where no profile directory is named. A value that is not an absolute path
/// names nothing: the profile directory answers, and no directory is made where the script started.
#[test]
fn the_record_goes_in_the_directory_a_person_named() {
    let Some(tools) = setup() else {
        return;
    };
    let release = brave("named-home");
    let install = Install::new(tools, "named-home");
    let named = install.root.join("named-state");
    let install = install.with_environment("BRAVEBOT_HOME", &named.display().to_string());
    let outcome = install.run_release(&release);
    assert_installed(&install, &outcome, &release);
    assert_eq!(
        read(&named.join("installed-by-powershell")),
        Some(format!("{}\n", install.bin().join(BINARY).display()).into_bytes()),
        "the named directory did not hold the record"
    );
    assert_eq!(
        read(&install.record()),
        None,
        "the record went under the profile directory as well as the named one"
    );

    let install = Install::new(tools, "named-home-alone").with_profile(Profile::Neither);
    let named = install.root.join("named-state");
    let install = install.with_environment("BRAVEBOT_HOME", &named.display().to_string());
    let outcome = install.run_release(&release);
    assert_installed(&install, &outcome, &release);
    assert!(
        read(&named.join("installed-by-powershell")).is_some(),
        "the named directory did not answer where no profile directory is named"
    );

    for passed_over in ["", "relative-state", "./relative-state"] {
        let install = Install::new(tools, "named-home-relative")
            .with_environment("BRAVEBOT_HOME", passed_over);
        let outcome = install.run_release(&release);
        assert_installed(&install, &outcome, &release);
        assert_eq!(
            read(&install.record()),
            Some(format!("{}\n", install.bin().join(BINARY).display()).into_bytes()),
            "{passed_over:?} was not passed over for the profile directory"
        );
        assert_eq!(
            outcome.in_working_directory,
            Vec::<PathBuf>::new(),
            "{passed_over:?} was taken as a place to write, relative to where the script started"
        );
    }
}

/// RELEASE-13: nothing outside the script chooses where it downloads from. Every request is to
/// the one repository, whatever the environment says and whatever it is called, and the script's
/// source reads only the variables it is allowed to: reading any other is how an origin would be
/// supplied, and a test can only try the names it thinks of.
#[test]
fn nothing_the_environment_names_chooses_where_the_script_downloads_from() {
    let Some(tools) = setup() else {
        return;
    };
    let mut install = Install::new(tools, "origin");
    for name in [
        "REPO",
        "REPOSITORY",
        "GITHUB_REPOSITORY",
        "BRAVEBOT_REPO",
        "BASE_URL",
        "API_URL",
        "RELEASE_URL",
        "DOWNLOAD_URL",
        "TAG",
        "VERSION",
        "BRAVEBOT_VERSION",
    ] {
        install = install.with_environment(name, "evil/repo");
    }
    let release = brave("origin");
    let outcome = install.run_release(&release);
    assert_installed(&install, &outcome, &release);
    for request in &outcome.requests {
        assert!(
            request.starts_with("https://api.github.com/repos/brave/bravebot/releases/latest")
                || request
                    .starts_with("https://github.com/brave/bravebot/releases/download/v0.0.0/"),
            "a request left the fixed repository: {request}"
        );
    }

    let source = std::fs::read_to_string(workspace().join("install.ps1")).expect("read the script");
    let allowed = [
        "HOME",
        "USERPROFILE",
        "BRAVEBOT_HOME",
        "INSTALL_DIR",
        "BRAVEBOT_NO_MODIFY_PATH",
        "LOCALAPPDATA",
        "PATH",
        "PROCESSOR_ARCHITEW6432",
        "PROCESSOR_ARCHITECTURE",
    ];
    let code: String = source
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let mut rest = code.as_str();
    while let Some(at) = rest.to_lowercase().find("$env:") {
        let name: String = rest[at + 5..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        assert!(
            allowed.contains(&name.as_str()),
            "the script reads ${{env:{name}}}, which is not one it is allowed to"
        );
        rest = &rest[at + 5..];
    }
    // Every `env:` is the start of a `$env:` read the loop above named, so a read by `${env:X}`,
    // an `Env:` drive or `Get-ChildItem env:` has no `$` in front of it and is counted here.
    let lowered = code.to_lowercase();
    assert_eq!(
        lowered.matches("env:").count(),
        lowered.matches("$env:").count(),
        "the script reads the environment another way than $env:NAME"
    );
    assert!(
        !code.contains("GetEnvironmentVariable"),
        "the script reads the environment through .NET"
    );
}

/// The writes the script made to the user's stored environment.
fn registry_writes(outcome: &Outcome) -> Vec<&str> {
    outcome
        .registry
        .iter()
        .map(String::as_str)
        .filter(|line| line.starts_with("write "))
        .collect()
}

/// `PATH` as a session that had `before` and gained `directory` has it.
fn path_with(before: &str, directory: &Path) -> String {
    format!("{before}:{}", directory.display())
}

/// A directory the session already finds is left where it is: nothing is read from or written to
/// the person's stored `Path`, and the session's own `PATH` is not extended with a second copy. A
/// script that wrote on every run would put the directory in the stored value once for each
/// update.
#[test]
fn a_directory_the_session_already_finds_is_left_alone() {
    let Some(tools) = setup() else {
        return;
    };
    let release = brave("found");
    let install = Install::new(tools, "path-found").with_stored_path("/opt/one");
    let outcome = install.run_release(&release);
    assert_installed(&install, &outcome, &release);
    assert!(
        outcome.output.contains("Run: bravebot --help"),
        "{}",
        outcome.output
    );
    assert_eq!(outcome.registry, Vec::<String>::new());
    assert_eq!(outcome.stored_path_after.as_deref(), Some("/opt/one"));
    assert_eq!(outcome.session_path, outcome.session_path_before);
}

/// A directory that is not on `PATH` is put at the end of the user's stored `Path` and the
/// session's, and says so, so that `bravebot` runs by name without the person finding out how
/// Windows keeps a `PATH` first ([USE-001]).
///
/// The stored value holds a `%NAME%` reference, and what has to come back is the text the person
/// had with the directory after it, written as an expandable string: reading it expanded and
/// writing that back replaces the reference with what it names today, and writing it as a plain
/// string stops Windows expanding the rest. The directory goes last, so it never takes a name from
/// a program already on the `PATH`.
///
/// [USE-001]: ../../../docs/best-practices/usability.md
#[test]
fn the_directory_is_appended_to_the_users_path_and_to_the_session() {
    let Some(tools) = setup() else {
        return;
    };
    let release = brave("appended");
    let stored = "/opt/one;%BRAVEBOT_TEST_TOOLS%/two";
    let install = Install::new(tools, "path-appended")
        .off_path()
        .with_stored_path(stored)
        .with_environment("BRAVEBOT_TEST_TOOLS", "/opt/tools");
    let outcome = install.run_release(&release);
    assert_installed(&install, &outcome, &release);

    assert_eq!(
        outcome.stored_path_after,
        Some(format!("{stored};{}", install.bin().display()))
    );
    assert_eq!(outcome.stored_kind.as_deref(), Some("ExpandString"));
    assert_eq!(registry_writes(&outcome), ["write Path"]);
    assert_eq!(
        outcome.session_path,
        path_with(&outcome.session_path_before, &install.bin())
    );
    assert!(
        outcome.output.contains(&format!(
            "Added {} to your user PATH.",
            install.bin().display()
        )),
        "{}",
        outcome.output
    );
    assert!(
        outcome.output.contains("Run: bravebot --help"),
        "{}",
        outcome.output
    );
}

/// A directory already in the stored `Path` is not added a second time, whichever way the entry is
/// spelled: with other letters in the case, with a separator after it, or as a `%NAME%` reference
/// to it. The session still gains it, since it is the session that was started before the entry
/// was there. An entry compared as text would add the directory again, once per run.
#[test]
fn a_directory_already_on_the_users_path_is_not_added_again() {
    let Some(tools) = setup() else {
        return;
    };
    let release = brave("present");
    let bin = scratch_root().join("path-present").join("bin");
    let spellings = [
        ("exactly", bin.display().to_string()),
        ("in other case", bin.display().to_string().to_uppercase()),
        ("with a separator after it", format!("{}/", bin.display())),
        ("as a reference", "%BRAVEBOT_TEST_BIN%".to_owned()),
    ];
    for (how, entry) in spellings {
        let stored = format!("/opt/one;{entry};/opt/two");
        let install = Install::new(tools, "path-present")
            .off_path()
            .with_stored_path(&stored)
            .with_environment("BRAVEBOT_TEST_BIN", &bin.display().to_string());
        let outcome = install.run_release(&release);
        assert_installed(&install, &outcome, &release);

        assert_eq!(registry_writes(&outcome), Vec::<&str>::new(), "{how}");
        assert_eq!(
            outcome.stored_path_after.as_deref(),
            Some(stored.as_str()),
            "{how}"
        );
        assert_eq!(
            outcome.session_path,
            path_with(&outcome.session_path_before, &install.bin()),
            "{how}"
        );
        assert!(
            outcome.output.contains("Run: bravebot --help") && !outcome.output.contains("Added"),
            "{how}: {}",
            outcome.output
        );
    }
}

/// An entry that names a different directory is not taken for the installed one: a name the
/// installed directory's is a prefix of, a directory inside it, the directory that holds it, and the
/// same final name under another parent. Only case and a trailing separator are folded, so each of
/// these leaves the directory to be added. A comparison on a prefix, or on the last component alone,
/// would find the directory already there and add nothing.
#[test]
fn a_different_directory_on_the_users_path_does_not_stop_the_directory_being_added() {
    let Some(tools) = setup() else {
        return;
    };
    let release = brave("different");
    let bin = scratch_root().join("path-different").join("bin");
    let other = scratch_root().join("path-different-other").join("bin");
    let entries = [
        format!("{}x", bin.display()),
        bin.join("sub").display().to_string(),
        bin.parent().expect("a parent").display().to_string(),
        other.display().to_string(),
    ];
    let stored = entries.join(";");
    let install = Install::new(tools, "path-different")
        .off_path()
        .with_stored_path(&stored);
    let outcome = install.run_release(&release);
    assert_installed(&install, &outcome, &release);

    assert_eq!(registry_writes(&outcome), ["write Path"]);
    assert_eq!(
        outcome.stored_path_after,
        Some(format!("{stored};{}", install.bin().display()))
    );
}

/// A stored `Path` that is missing or empty becomes the directory alone, and one that ends in a
/// separator does not gain a second. Joining with a separator whatever came before leaves an empty
/// entry, which Windows reads as the current directory.
#[test]
fn an_empty_or_unterminated_users_path_is_joined_without_an_empty_entry() {
    let Some(tools) = setup() else {
        return;
    };
    let release = brave("joined");
    let cases: [(&str, Option<&str>); 3] = [
        ("path-missing", None),
        ("path-empty", Some("")),
        ("path-terminated", Some("/opt/one;")),
    ];
    for (name, stored) in cases {
        let mut install = Install::new(tools, name).off_path();
        if let Some(stored) = stored {
            install = install.with_stored_path(stored);
        }
        let outcome = install.run_release(&release);
        assert_installed(&install, &outcome, &release);
        let bin = install.bin().display().to_string();
        let expected = match stored {
            Some(stored) if !stored.is_empty() => format!("{stored}{bin}"),
            _ => bin,
        };
        assert_eq!(
            outcome.stored_path_after,
            Some(expected),
            "stored {stored:?}"
        );
    }
}

/// BRAVEBOT_NO_MODIFY_PATH, set to anything, keeps the script from touching the person's `Path` or
/// the session's. The install still happens, and the last line names the directory to add.
#[test]
fn a_person_who_opts_out_keeps_their_path_as_it_is() {
    let Some(tools) = setup() else {
        return;
    };
    let release = brave("opted-out");
    let install = Install::new(tools, "path-opted-out")
        .off_path()
        .with_stored_path("/opt/one")
        .with_environment("BRAVEBOT_NO_MODIFY_PATH", "1");
    let outcome = install.run_release(&release);
    assert_installed(&install, &outcome, &release);

    assert_eq!(outcome.registry, Vec::<String>::new());
    assert_eq!(outcome.stored_path_after.as_deref(), Some("/opt/one"));
    assert_eq!(outcome.session_path, outcome.session_path_before);
    assert!(
        outcome
            .output
            .contains(&format!("{} is not on your PATH", install.bin().display()))
            && outcome.output.contains("BRAVEBOT_NO_MODIFY_PATH"),
        "{}",
        outcome.output
    );
    assert!(
        !outcome.output.contains("Run: bravebot"),
        "{}",
        outcome.output
    );
}

/// A stored `Path` the person's account cannot change does not undo the install: the binary is in
/// place and recorded, and the script says it could not add the directory and which to add. Saying
/// it was added would send the person to a command that is not found.
#[test]
fn a_refused_registry_write_leaves_the_install_and_says_what_is_left() {
    let Some(tools) = setup() else {
        return;
    };
    let release = brave("denied");
    let install = Install::new(tools, "path-denied")
        .off_path()
        .with_stored_path("/opt/one")
        .with_registry_write_denied();
    let outcome = install.run_release(&release);
    assert_installed(&install, &outcome, &release);

    assert_eq!(outcome.stored_path_after.as_deref(), Some("/opt/one"));
    assert!(
        outcome.output.contains(&format!(
            "could not add {} to your user PATH",
            install.bin().display()
        )) && outcome.output.contains("Requested registry access"),
        "{}",
        outcome.output
    );
    assert!(
        !outcome.output.contains("Added") && !outcome.output.contains("Run: bravebot"),
        "{}",
        outcome.output
    );
    assert_eq!(outcome.session_path, outcome.session_path_before);
}

/// A directory a stored `Path` would misread is not written to it, and the script says which
/// character and leaves the install in place, the stored value and the session as they were. A
/// semicolon divides a `PATH`, so the directory would be two entries; a percent sign starts a
/// `%NAME%` reference, which Windows expands in a stored value, so a new terminal would look in a
/// different directory than the one installed to, and the next run would not find it there.
#[test]
fn a_directory_a_path_would_misread_is_not_put_on_it() {
    let Some(tools) = setup() else {
        return;
    };
    let release = brave("misread");
    for (name, leaf, character) in [
        ("path-semicolon", "bin;two", "semicolon"),
        ("path-percent", "bin%PCT_TEST%two", "percent sign"),
    ] {
        let directory = scratch_root().join(name).join(leaf);
        let install = Install::new(tools, name)
            .off_path()
            .with_stored_path("/opt/one")
            .installing_to(Some(directory.clone()));
        let outcome = install.run_release(&release);

        assert!(outcome.succeeded, "{character}: {}", outcome.output);
        assert_eq!(
            read(&directory.join(BINARY)).as_deref(),
            Some(&release[..]),
            "{character}"
        );
        assert_eq!(registry_writes(&outcome), Vec::<&str>::new(), "{character}");
        assert_eq!(
            outcome.stored_path_after.as_deref(),
            Some("/opt/one"),
            "{character}"
        );
        assert_eq!(
            outcome.session_path, outcome.session_path_before,
            "{character}"
        );
        assert!(
            outcome.output.contains("could not add") && outcome.output.contains(character),
            "{character}: {}",
            outcome.output
        );
        assert!(
            !outcome.output.contains("Added") && !outcome.output.contains("Run: bravebot"),
            "{character}: {}",
            outcome.output
        );
    }
}

/// An install that is refused changes nothing about the person's `Path` or the session's, since
/// the directory it would add holds no program. The directory is off the session's `PATH` and the
/// stored value is present, so a script that put the directory on either `PATH` before it checked
/// what it had downloaded would show it here.
#[test]
fn a_refused_install_leaves_the_users_path_alone() {
    let Some(tools) = setup() else {
        return;
    };
    let published = brave("published");
    let substitute = brave("substitute");
    let install = Install::new(tools, "path-refused")
        .off_path()
        .with_stored_path("/opt/one");
    let outcome = install.run(&[
        (&install.asset(), &substitute),
        (
            &format!("{}.sha256", install.asset()),
            &checksum(tools, &published),
        ),
    ]);
    assert_refused(&install, &outcome, "checksum mismatch");
    assert_eq!(outcome.stored_path_after.as_deref(), Some("/opt/one"));
    assert_eq!(outcome.session_path, outcome.session_path_before);

    let install = Install::new(tools, "path-refused-signer")
        .off_path()
        .with_stored_path("/opt/one");
    let outcome = install.run_release(&binary(Some("Valid"), Some("Someone Else")));
    assert_refused(&install, &outcome, "not signed by");
    assert_eq!(outcome.stored_path_after.as_deref(), Some("/opt/one"));
    assert_eq!(outcome.session_path, outcome.session_path_before);
}

/// A destination the script cannot write or replace is refused with the reason, and what was
/// there is left as it was. The destination is a directory holding a file, which neither moving a
/// copy onto it nor replacing it can succeed at, whichever system the test runs on; on Windows a
/// running executable fails the same call. The staged copy made beside it is removed, nothing is
/// recorded, and the person's `Path` and the session's are not touched.
#[test]
fn a_destination_that_cannot_be_replaced_is_refused_and_left_as_it_was() {
    let Some(tools) = setup() else {
        return;
    };
    let release = brave("new");
    let install = Install::new(tools, "unreplaceable")
        .off_path()
        .with_stored_path("/opt/one");
    let destination = install.bin().join(BINARY);
    std::fs::create_dir_all(&destination).expect("make a directory where the executable goes");
    let old = destination.join("old");
    std::fs::write(&old, b"old copy").expect("put a file in it");

    let outcome = install.run_release(&release);

    assert!(!outcome.succeeded, "installed: {}", outcome.output);
    assert!(
        outcome.output.contains("unable to write")
            && outcome.output.contains("close it and run this again"),
        "refused for another reason: {}",
        outcome.output
    );
    assert!(
        !outcome.output.contains("Installed bravebot"),
        "{}",
        outcome.output
    );
    assert!(destination.is_dir(), "the destination was replaced");
    assert_eq!(
        read(&old).as_deref(),
        Some(&b"old copy"[..]),
        "what was at the destination changed"
    );
    assert_eq!(
        entries(&install.bin()),
        vec![destination.clone()],
        "a staged copy or another file was left beside the destination"
    );
    assert_eq!(
        entries(&destination),
        vec![old],
        "something was written into the directory at the destination"
    );
    assert_eq!(
        read(&install.record()),
        None,
        "a refused install recorded where it went"
    );
    assert_eq!(
        outcome.left_in_temporary,
        Vec::<String>::new(),
        "the download was left behind"
    );
    assert_eq!(
        outcome.registry,
        Vec::<String>::new(),
        "a refused install read or changed the stored environment"
    );
    assert_eq!(outcome.stored_path_after.as_deref(), Some("/opt/one"));
    assert_eq!(outcome.session_path, outcome.session_path_before);
}
