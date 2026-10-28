---
sidebar_position: 1.5
title: Install
description: Install, update and uninstall Brave Bot, and check a download by hand.
---

# Install

npm installs the binary for your platform from its own package, `@brave/bravebot-<platform>-<arch>`,
and runs no install script, so `--ignore-scripts` does not affect it. `--omit=optional` leaves that
package out, and then `bravebot` exits saying the binary is missing. macOS, Linux and Windows are
supported, on both x86_64 and arm64. To build from source instead, see
[Development](development.md).

On macOS and Linux there is an install script, for a machine with no npm on it:

```sh
curl -fsSL https://raw.githubusercontent.com/brave/bravebot/main/install.sh | sh
```

It fetches the newest release for your platform, checks it against the published checksum, and
writes nothing if the two differ. It puts the binary in `/usr/local/bin`, asking for sudo only if
that directory is not yours to write to. `INSTALL_DIR` puts it somewhere else:

```sh
curl -fsSL https://raw.githubusercontent.com/brave/bravebot/main/install.sh \
  | INSTALL_DIR="$HOME/.local/bin" sh
```

To install a release other than the newest, give the script a version after `-s`. It takes three
numbers, with or without a leading `v`, and checks that release exactly as it checks the newest:

```sh
curl -fsSL https://raw.githubusercontent.com/brave/bravebot/main/install.sh | sh -s 1.2.3
```

A version with no release fails at the download and says which one. Running the line without a
version installs the newest again.

Running the line again is how an install made this way is updated. The script records where it put
the binary, so a second run with no directory named replaces that one rather than leaving two on
your `PATH`.

## Debian, Ubuntu and Fedora packages

On Debian 12, Ubuntu 22.04, Ubuntu 24.04 and Fedora 42, `bravebot` is a package of its own, in an
apt repository and a dnf repository. Both are signed with the release key whose fingerprint is
`13F28F0405C49B0B232DBA1BC1E827646A2DE416`, and the client checks that signature before it
installs anything. Other releases that provide glibc 2.30 or newer run the package too, and are not
supported. The package puts the binary at `/usr/bin/bravebot` and depends on nothing else.

For apt, fetch the key to the place the source names, then add the source:

```sh
sudo curl -fsSL -o /usr/share/keyrings/bravebot-release.asc \
  https://brave-browser-downloads.s3.brave.com/keys/bravebot-release.asc
gpg --show-keys --with-fingerprint /usr/share/keyrings/bravebot-release.asc
sudo tee /etc/apt/sources.list.d/bravebot.sources <<'EOF'
Types: deb
URIs: https://brave-browser-downloads.s3.brave.com/bravebot/apt/
Suites: ./
Signed-By: /usr/share/keyrings/bravebot-release.asc
EOF
sudo apt update
sudo apt install bravebot
```

The fingerprint `gpg` prints has to be the one above. `Signed-By` makes apt accept this
repository only when that key signed it.

For dnf:

```sh
sudo tee /etc/yum.repos.d/bravebot.repo <<'EOF'
[bravebot]
name=Brave Bot
baseurl=https://brave-browser-downloads.s3.brave.com/bravebot/rpm/
enabled=1
gpgcheck=0
repo_gpgcheck=1
gpgkey=https://brave-browser-downloads.s3.brave.com/keys/bravebot-release.asc
EOF
sudo dnf install bravebot
```

The first install asks whether to import the key, and shows its fingerprint. Accept only the one
above. `repo_gpgcheck=1` makes dnf check the signature on the repository's metadata, which holds the
hash of each package. The `.rpm` files carry no signature of their own, so `gpgcheck` is `0`.

A package installed this way is updated with `sudo apt update && sudo apt upgrade bravebot` or
`sudo dnf upgrade bravebot`. The startup notice does not recognise a copy in `/usr/bin`, so it
says nothing about newer releases for one.

On Windows there is a PowerShell script, for a machine with no Node on it. It needs no
Administrator:

```powershell
irm https://raw.githubusercontent.com/brave/bravebot/main/install.ps1 | iex
```

It fetches the newest release for the architecture your PowerShell reports, checks it against the
published checksum, and checks that the binary's Authenticode signature is valid and was made by `Brave
Software, Inc.`. It writes nothing if either check fails. It puts `bravebot.exe` in
`%LOCALAPPDATA%\Programs\bravebot`, and `INSTALL_DIR` puts it somewhere else:

```powershell
$env:INSTALL_DIR = "$HOME\bin"; irm https://raw.githubusercontent.com/brave/bravebot/main/install.ps1 | iex
```

It adds the directory to the end of your user `PATH`, keeping every other entry as it was, and to
the PowerShell session it ran in, so `bravebot` runs by name there at once and in terminals you open
afterwards. A directory already on your `PATH` is not added again. To keep your `PATH` as it is, set
`BRAVEBOT_NO_MODIFY_PATH` to anything before the line, and the script names the directory for you to
add yourself:

```powershell
$env:BRAVEBOT_NO_MODIFY_PATH = "1"; irm https://raw.githubusercontent.com/brave/bravebot/main/install.ps1 | iex
```

To install a release other than the newest, run the script as a script block and give it a version.
It takes three numbers, with or without a leading `v`, and checks that release exactly as it checks
the newest:

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/brave/bravebot/main/install.ps1))) 1.2.3
```

Running the first line again is how an install made this way is updated. The script records where it
put the binary in `.bravebot\installed-by-powershell` under your profile directory, so a second run
with no directory named replaces that one. Windows may not let a `bravebot.exe` that is running be
replaced. The script then installs nothing and says so: close every session of it and run the line
again. The script needs PowerShell on Windows, since the signature check is
`Get-AuthenticodeSignature`, and in any other PowerShell it installs nothing.

## Check a download by hand

The installers check the binary before they write it, and refuse when it does not verify. To check
a release yourself, download the asset for your platform from the
[releases page](https://github.com/brave/bravebot/releases), then:

On macOS, the binary has to be signed by Brave's Developer ID team (`KL8N8XSYF4`) and accepted by
Gatekeeper. Both commands print nothing when they succeed:

```sh
codesign --verify --deep --strict \
  -R '=anchor apple generic and certificate leaf[subject.OU] = "KL8N8XSYF4"' bravebot-darwin-arm64
spctl -a -t install bravebot-darwin-arm64
```

On Windows, in PowerShell, `Status` has to be `Valid` and the signer `Brave Software, Inc.`:

```powershell
Get-AuthenticodeSignature .\bravebot-windows-amd64.exe |
  Format-List Status, @{ n = 'Signer'; e = { $_.SignerCertificate.GetNameInfo('SimpleName', $false) } }
```

On Linux the binary carries no signature. Its `.sha256` file is signed, and the signature has to
be from the key whose fingerprint is `13F28F0405C49B0B232DBA1BC1E827646A2DE416`. The installers carry
that key themselves; the same key is published at
`https://brave-browser-downloads.s3.brave.com/keys/bravebot-release.asc`. With the asset, its
`.sha256` and its `.sha256.asc` in the current directory:

```sh
export GNUPGHOME="$(mktemp -d)"
curl -fsSL https://brave-browser-downloads.s3.brave.com/keys/bravebot-release.asc | gpg --import
gpg --fingerprint 13F28F0405C49B0B232DBA1BC1E827646A2DE416
gpg --verify bravebot-linux-amd64.sha256.asc bravebot-linux-amd64.sha256
echo "$(cat bravebot-linux-amd64.sha256)  bravebot-linux-amd64" | sha256sum --check
```

`gpg --verify` has to say `Good signature` from a key with that fingerprint, and `sha256sum` has
to say `OK`.

## Check the configuration

Configuration is baked into the released binary, so there is nothing to set up. Check what it will
actually use:

```sh
bravebot doctor
```

```
configuration OK
  endpoint  https://ai-chat.bsg.brave.com/v1/chat/completions
  premium   https://ai-gateway-api.bsg.brave.software/v1/chat/completions, without a premium key, so requests go to the endpoint above
  key id    …
  model     automatic-bravebot (default)
  key       … (never transmitted)

state directory ~/.bravebot, from HOME

confinement …

network
  trust roots  built in (SSL_CERT_FILE, SSL_CERT_DIR names others)
  proxy        none (HTTPS_PROXY, HTTP_PROXY names one, in upper case or lower)
```

It names the state directory it resolved, or says there is none, why, and what is not kept without
one: with no state directory there are no settings of your own, no session to resume, no prompt
history and no skills. The `network` section names the trust roots in force, the proxy, and the hosts
it is not used for, which is the first place to look when every request fails with an unknown issuer.

A build with no model service configured does not start work at all. It says so and lists the ways to
set one up, each naming what to type or write, rather than running against an endpoint that will not
serve it. `doctor` says the same and fails. See
[Configuration](customize/configuration.md).

## Staying current

A session that opens on a version something newer has replaced says so, once, under the trust
question, and gives the line that updates the copy you are running. The notice names the version you
are running rather than the newer one, which may have moved by the time you update: the npm command for an npm
install, the script again for a script install, the PowerShell script again for a PowerShell script
install, `brew upgrade --cask bravebot` for a copy a Homebrew cask installed and `winget upgrade
Brave.Bravebot` for one WinGet installed. A package manager's copy is recognised by the directory the
manager keeps it in. A build from source is told nothing,
since none of those lines would update one. A package can arrive after the release it follows, and
until it does the manager says there is nothing to upgrade.

Nothing about it waits. The notice comes from an answer an earlier launch wrote down, and the
request that refreshes it, at most one an hour whether or not it learned anything, runs behind the session
and is for the next one. So a
first run says nothing, and a release is announced on the launch after the one that first asks for it.

Every way this can fail is silence: no network, a registry that will not answer, an answer of an
unexpected shape. A registry that cannot be reached today does not withdraw a version it announced
earlier, so the notice keeps showing until the registry answers with something else. A version that is not three numbers is never announced either, release candidates
included. An [incognito session](using/incognito.md) still
reads an answer an ordinary session left, and neither records one nor asks. A launch that cannot
write the record of its last check, because there is no home directory or the state directory is not
writable, does not ask the registry at all.

## Uninstall

Remove it the way it was installed.

An apt or dnf install goes with the package manager. `~/.bravebot` is kept, as for every route:

```sh
sudo apt remove bravebot
sudo dnf remove bravebot
```

An npm install goes with npm:

```sh
npm uninstall -g @brave/bravebot
```

A script install is one binary, and the script wrote down where it put it:

```sh
cat ~/.bravebot/installed-by
```

That file holds the full path. It is `/usr/local/bin/bravebot` unless `INSTALL_DIR` named somewhere
else, and a machine with no home directory has no record to read. Remove that path, with `sudo` where
the directory is not yours to write to:

```sh
sudo rm /usr/local/bin/bravebot
```

A PowerShell script install is one binary as well, and the script wrote down where it put it:

```powershell
Get-Content "$env:USERPROFILE\.bravebot\installed-by-powershell"
```

The file is under the directory `HOME` names where that is set, which is where bravebot keeps its
state directory too, so use `$env:HOME` in its place there. It holds the full path of `bravebot.exe`, which is under `%LOCALAPPDATA%\Programs\bravebot`
unless `INSTALL_DIR` named somewhere else. Remove `bravebot.exe` at that path, and the directory
that held it if nothing else is in it. The script added that directory to your user `PATH` unless
`BRAVEBOT_NO_MODIFY_PATH` was set, so remove that entry too.

No route touches `~/.bravebot`. It holds your settings, prompt history, session records, themes,
MCP declarations and any stored sign-ins, so removing the binary leaves all of that for a later
install to find. To remove it too:

```sh
rm -rf ~/.bravebot
```

Two things are added by hand and are removed by hand: an `eval "$(bravebot completion …)"` or
`eval "$(bravebot shell-init …)"` line in `~/.bashrc`, `~/.zshrc` or `~/.config/fish/config.fish`,
and `~/.config/fish/completions/bravebot.fish` where a fish completion was written there. See
[`completion`](reference/cli.md#completion) and [`shell-init`](reference/cli.md#shell-init).
