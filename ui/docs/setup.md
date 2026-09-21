# Setup

## Prerequisites

- **macOS or Linux**. macOS needs Xcode Command Line Tools (`xcode-select --install`).
  Linux needs a C/C++ compiler and linker (for example, `base-devel` on Arch or
  `build-essential` on Debian/Ubuntu), plus a graphical desktop and Electron's
  GTK 3, NSS, and ALSA runtime libraries. The window uses native Linux decorations;
  inset traffic lights and sidebar vibrancy are enabled only on macOS.
- **Current stable Rust**, preferably installed with rustup. `rust-toolchain.toml`
  selects stable and Clippy; `rustup update stable` updates an existing installation.
  The workspace declares Rust 1.88 as its minimum.
- **Node 22.12+ and npm**. CI uses Node 24. The app uses Electron 44 and React 19.
- **Git**, including submodule support.
- **Optional: direnv**, for loading backend credentials. It is unnecessary for
  builds without credentials or when the required variables are already exported.

## Clone and run

```bash
git clone --recurse-submodules https://github.com/brave-experiments/brave-bot-ui.git
cd brave-bot-ui
npm ci
npm run dev
```

`npm ci` installs the locked dependency versions and runs Electron runtime setup.
`npm run dev` builds both Rust executables and starts Electron with hot reload.
To build and preview without hot reload, see [development](development.md).

For a clone made without submodules:

```bash
git submodule update --init --recursive
```

## Credentials

A fresh checkout builds without secrets and can display saved sessions. Inference
requires backend configuration. For the Brave backend, supply these exported names:

| Variable | Purpose |
| --- | --- |
| `SERVICES_KEY_AICHAT` | Backend signing key |
| `BRAVE_SERVICES_KEY_ID` | Key identifier |
| `BRAVE_AI_CHAT_ENDPOINT` | Backend endpoint |
| `BRAVE_AI_CHAT_DEFAULT_MODEL` | Optional default model; otherwise `automatic` |
| `BRAVE_AI_CHAT_PREMIUM_ENDPOINT` | Optional premium endpoint |

Use the exported names, not the `DEV_`/`PROD_` inputs used by the agent's example
configuration. Keep the values outside the checkout, for example in
`~/.config/bravebot/env`, as `NAME=value` lines. Create it with your editor and set
its permissions to 0600. Do not commit the values.

With direnv installed, add this line to a `.envrc` in the UI repository (preserve
any existing contents):

```bash
dotenv_if_exists ~/.config/bravebot/env
```

After reviewing that file, allow it and explicitly run the build through it:

```bash
direnv allow .
direnv exec . npm run build
npm start
```

For hot reload with the same environment, use `direnv exec . npm run dev`.
A shell with direnv integration can load these variables automatically; `direnv allow`
alone does not export them into a shell without that integration.

The agent's configuration build script bakes available Brave backend values into
`bravebot-rpc`. Runtime environment values override baked ones. Keeping a credential
file outside Git lets multiple checkouts use it and keeps it through re-clones.
The file is not automatically discovered: the `.envrc` above is what loads it.

Alternatively, `scripts/build-bridge.sh` loads an allowed `.envrc` from
`vendor/bravebot` via direnv. `BRAVEBOT_DIR` (or the legacy `BUA_AGENT_DIR`) can point
to a different credential checkout. These variables affect credential loading only;
Cargo still compiles `vendor/bravebot`. The script canonicalises the directory for
direnv's allow list. It first uses an already-exported `SERVICES_KEY_AICHAT` when present.

### Builds without credentials

`.cargo/config.toml` sets `BRAVEBOT_ALLOW_UNCONFIGURED_BUILD=1`, allowing missing
credentials during development and CI. This does not prevent baking values that
are present. Missing credentials therefore do **not** make this checkout's build
fail; they prevent inference at runtime. The app offers backend diagnostics and setup help.

For a Brave-backend build that must fail if required values are missing, run:

```bash
direnv exec . env BRAVEBOT_ALLOW_UNCONFIGURED_BUILD=0 npm run build
```

An app started from a configured shell inherits its environment; Finder does not
load your shell's `.envrc`. A distributable Brave-backend app needs the required
configuration baked in. See [packaging](development.md#packaging).
The agent also supports runtime Bedrock configuration; its variable definitions are
in [`env_var.rs`](../vendor/bravebot/crates/config/src/env_var.rs). Bedrock/AWS values
are not baked into the binary by this build script.

## Updating an existing checkout

After pulling changes, synchronise any changed URL and check out the committed pin:

```bash
git pull --ff-only
git submodule sync --recursive
git submodule update --init --recursive
npm ci
```

`vendor/bravebot` points to [brave/bravebot](https://github.com/brave/bravebot),
currently at v0.9.0 (`c23b2ed`). A URL update alone does not change that revision.
`npm run bridge` warns if the checked-out submodule differs from the pin.

### Intentionally updating the pin

The bridge depends on upstream internals by path. Pinning makes an incompatible
upstream change arrive in a reviewed update rather than during an unrelated build.
Choose an explicit revision, then run the [submodule-update checks](testing.md#current-regression-checks)
and review any lockfile changes before committing:

```bash
git -C vendor/bravebot fetch origin
# Replace <revision> with the intended tag or commit.
git -C vendor/bravebot checkout <revision>
npm run build
cargo test --all
git add vendor/bravebot Cargo.lock
git commit -S -m "Update the bravebot submodule pin"
```

CI checks the committed revision. Commits intended for `main` must have verified
signatures under the repository rules.

## Electron runtime troubleshooting

Electron 44's npm package needs a separate runtime download. The repository's
postinstall runs that installer and names the development app **Brave Bot**.
Development and preview commands also run setup and reuse an installed runtime.

If you see `Error: Electron uninstall`, or installed with `--ignore-scripts`, run:

```bash
npm run setup:electron
npm run build
npm start
```

Unset `ELECTRON_SKIP_BINARY_DOWNLOAD` before launching. CI sets it to `1` because
its TypeScript check does not need an Electron executable. If installation fails,
check the installer output and retry setup after fixing the download problem.
