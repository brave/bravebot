#!/usr/bin/env sh
#
# Installs the newest bravebot release for this platform, or the one named by the version given.
#
#   curl -fsSL https://raw.githubusercontent.com/brave/bravebot/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/brave/bravebot/main/install.sh | sh -s 1.2.3
#   curl -fsSL https://raw.githubusercontent.com/brave/bravebot/main/install.sh | INSTALL_DIR="$HOME/.local/bin" sh
#
# The checksum check is not optional: without it a network-fetched executable would run on the
# strength of TLS alone, and a substituted release asset would be indistinguishable from a good
# one. Running this again is also how an install made this way is updated, so it writes down where
# the binary went: that is what lets bravebot name the command that updates this copy, and what
# puts a later run in the same place rather than beside it.
#
# Nor is the signature check. The checksum is published beside the binary, so whoever can replace
# one can replace both. On Linux the checksum carries a signature that has to verify against the key
# embedded below (skipped, with a note, when gpg is not installed). On macOS the binary has to be
# signed by Brave's Developer ID team and pass Gatekeeper's assessment.

set -eu

REPO="brave/bravebot"
API_URL="https://api.github.com/repos/${REPO}/releases/latest"
# The key a Linux checksum has to be signed by. This is the public half, embedded so it has no
# external dependency. The fingerprint ensures only this specific key is trusted.
SIGNING_KEY_FINGERPRINT="13F28F0405C49B0B232DBA1BC1E827646A2DE416"
# The Developer ID team that signs each macOS binary. codesign alone accepts a valid signature from
# any team Apple issued a certificate to, so a signature from another team is a refusal.
APPLE_TEAM_ID="KL8N8XSYF4"
BIN_NAME="bravebot"
DEFAULT_INSTALL_DIR="/usr/local/bin"
RELEASE_PUBKEY="$(cat <<'PUBKEY'
-----BEGIN PGP PUBLIC KEY BLOCK-----

mQINBGqsIu8BEACplre+QZwgCtdzPGBFhzPQFpxifwb5qWOnVcZ4yWmlCUpBYx8D
45miyE6T6jKl7pzqHKtvQHfOAsb0NoRmKSELSCfyGHCXkU9Bpd0KSae9HAJKLlD6
HhnFnG1gyQK78kac3vWhqIjKYg5RKP0fWMtPhkDG7+YishzbbJ7E37gyvQW3JhnB
oXGx1C1XlKh1K1sr2N5XDCIDsH0/jP8nzsX5d9+1FkrgSHoXmmZyUPJ6mKADl+D1
JgqZe0X135qva0+dBYa/bbB9nqWBR+aOYYnuVs1UPFBb9fFAwK3o7t83S4gobf3d
joGjoFk9exVKwnUtXWmIyxH98Zl6ASJJuVCrvA3qHI7qqiFrQe/1JkEjW3+36iRJ
80NNJCRLgzt/QhOKEUDnt4kg1+g2nwNGmLFajzXmcFa4/u9ANOOfcAWErmf/w218
nu2syB8pH6QFYcezec8/WVgCN83bNRLwb1nVgYQ0IXu2+XDsqFtfcEGGcCM2ASoH
cdmQggcpGg6+8G8xh+wnhu/ufn3m8FJXCpk7uHwg+yRM5lhUQDMMA6zHPULlX+P5
cgtqCUVu9sxr8sfIc4o+oQ2vZC+8FbUWBx80yLp8eXrCImMOUgS0sGSZ/6WA0/sI
1SPUIc9UlpWXTGEFnZCxO7dmzKcXtsqhRxLHgki5rVSHWydmzRNDdzC/9wARAQAB
tEJCcmF2ZSBCb3QgUmVsZWFzZSAoQnJhdmUgQm90IFJlbGVhc2UpIDxicmF2ZWJv
dC1yZWxlYXNlQGJyYXZlLmNvbT6JAoAEEwEKAGoCGwMFCRLMAwAFCwkIBwICIgIG
FQoJCAsCBBYCAwECHgcCF4AWIQQT8o8EBcSbCyMtuhvB6Cdkai3kFgUCaqwi8SgU
gAAAAAAPABBsYWJlbEBicmF2ZS5jb21icmF2ZWJvdF9yZWxlYXNlAAoJEMHoJ2Rq
LeQW0pMP/29I7HaVbj0uu/YlsMTSjEdwaZjJ+hxItW5zxTsnCIormoJAVjNT0tar
HLjz11QwnRwcYnu9nEeZiIVMUQ5ugoOQZZ4blVHSgiTJCUDcdcmCP+p9knlUFzP1
nL23RS1tt5rGdsJw+SWNnxl4A8Ako29KhtPISdxAV5sM+8ZX07ONTEsEYE2FzqZt
W1ylZsV4hYuSnL/wIXgYvVyo7ME+DlSel5vXGHmQHMIe2dKR66ErlHCcRpVS3rqv
4ZTMpS/w3Fy5T5cJody37R1JRSfFfGvSsF7SGMbQ5tT3o5Y5aEW8+V5MbluH5Fns
v2gfCObXrJmG+fs0rlC4Qy/taKi4HC44XJy7kIhcI2d8/OH2wJgu0JTAmk2OMxD+
/nbD1VQXkAqKS2NEMlXm7QsNxjEi+fUquzPc9F0MmjM5+Y8jEwJpDidPdmGGRVdn
EVOksqVLyJDmuFIbChfUyMblgDbnLjqP6hOiOvWqyfsirr1MOXaZ6igAeokE0c3y
s9HGBsRJA682nL1rZQ+OcvQigiVm6Eo/L1t5KM/Luz4+65lRJVy2i9YliDHimP6U
SEWQlP6FAOZEwtoP0KVOy5cJcfUTY1hiOvtiqGyq+hi3PRk9JTunFjue/Q+17epr
HNzPcU6aUBlTlXbHvSS1MSD2AQ1aKX3mYpkde0A0Aaw3/J82Zpz3
=FyJl
-----END PGP PUBLIC KEY BLOCK-----
PUBKEY
)"

# An unset or empty HOME names no profile directory, and then there is no state directory at all:
# nothing is read from one and nothing written to one (docs/specs/state-directory.md#STATE-2).
# Joining the name onto nothing makes `/.bravebot`, a location nobody stated, and a record under it
# would decide where this release lands.
STATE_DIR=""
INSTALLED_BY=""
if [ -n "${HOME:-}" ]; then
  STATE_DIR="${HOME}/.bravebot"
  INSTALLED_BY="${STATE_DIR}/installed-by"
fi

fail() {
  echo "error: $1" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

# Three decimal numbers joined by dots, and nothing else. The version becomes part of a URL, so it is
# held to this shape rather than to "does not look dangerous": a dot-dot, a slash or a host in it
# is refused here, before any request, and what is left can only name a tag under REPO.
is_version() {
  case "$1" in
    "" | *[!0-9.]* | .* | *. | *..*) return 1 ;;
  esac
  # With no empty field possible, two dots is exactly three numbers.
  case "$1" in
    *.*.*) ;;
    *) return 1 ;;
  esac
  rest="${1#*.}"
  case "${rest#*.}" in
    *.*) return 1 ;;
  esac
  return 0
}

# The digest and nothing else: sixty-four hex digits, no filename beside them. A checksum file of
# any other shape is a release published wrong, and installing anyway would make the one signal
# that distinguishes a bad download from a good one meaningless.
is_sha256() {
  case "${#1}" in
    64) ;;
    *) return 1 ;;
  esac
  case "$1" in
    *[!0-9a-fA-F]*) return 1 ;;
  esac
  return 0
}

# Succeeds only when the signature verifies and was made by the key with the given fingerprint.
# A good signature from any other key in the imported file is a refusal, which is what the
# fingerprint is for. The keyring is made for this call and removed after it, so the person's own
# is neither read nor added to.
verify_checksum_signature() {
  asc_path="$1"
  sha_path="$2"
  pubkey_path="$3"
  fingerprint="$4"
  gpg_home="$(mktemp -d)" || return 1
  # The last field of VALIDSIG is the primary key's fingerprint, whichever subkey signed.
  if (
    GNUPGHOME="$gpg_home"
    export GNUPGHOME
    gpg --batch --quiet --import "$pubkey_path" 2>/dev/null || exit 1
    status="$(gpg --batch --status-fd 1 --verify "$asc_path" "$sha_path" 2>/dev/null)" || exit 1
    printf '%s\n' "$status" | grep -q "^\[GNUPG:\] VALIDSIG .* ${fingerprint}\$"
  ); then
    rc=0
  else
    rc=1
  fi
  rm -rf "$gpg_home"
  return "$rc"
}

# The requirement holds the signature to a certificate Apple issued to APPLE_TEAM_ID, and spctl
# is the Gatekeeper assessment a double-click would get, which is where notarization is checked.
# A shell started from a terminal is never assessed by Gatekeeper, so this asks for it. `-t install`
# because the asset is a bare executable: `-t execute` rejects anything that is not an app bundle,
# however it is signed.
verify_code_signature() {
  codesign --verify --deep --strict \
    -R "=anchor apple generic and certificate leaf[subject.OU] = \"${APPLE_TEAM_ID}\"" "$1" &&
    spctl -a -t install "$1"
}

main() {
  [ "$#" -le 1 ] || fail "usage: install.sh [version]"
  # Checked first, so an argument that is refused costs no request and writes nothing.
  TAG=""
  if [ "$#" -eq 1 ]; then
    requested="${1#v}"
    is_version "$requested" || fail "not a version: expected three numbers such as 1.2.3 (a leading v is allowed)"
    TAG="v${requested}"
  fi

  need_cmd curl
  need_cmd mktemp
  need_cmd chmod
  need_cmd mv
  need_cmd rm
  need_cmd uname

  case "$(uname -s)" in
    Darwin) OS_KEY="darwin" ;;
    Linux) OS_KEY="linux" ;;
    *) fail "unsupported OS: $(uname -s). Windows installs with npm: npm install -g @brave/bravebot" ;;
  esac

  case "$(uname -m)" in
    arm64 | aarch64) ARCH_KEY="arm64" ;;
    x86_64 | amd64) ARCH_KEY="amd64" ;;
    *) fail "unsupported architecture: $(uname -m)" ;;
  esac

  # Under Rosetta a translated shell reports x86_64 on an arm64 machine. The x86_64 binary would
  # work and would run translated, so take the native one.
  if [ "$OS_KEY" = "darwin" ] && [ "$ARCH_KEY" = "amd64" ]; then
    translated="$(sysctl -in sysctl.proc_translated 2>/dev/null || true)"
    native_arm="$(sysctl -in hw.optional.arm64 2>/dev/null || true)"
    if [ "$translated" = "1" ] && [ "$native_arm" = "1" ]; then
      echo "Detected Rosetta translation; installing the native arm64 binary."
      ARCH_KEY="arm64"
    fi
  fi

  ASSET_NAME="${BIN_NAME}-${OS_KEY}-${ARCH_KEY}"

  # Asked before anything is downloaded, so a Mac that cannot check a signature is told so rather
  # than handed a binary nobody vouched for.
  if [ "$OS_KEY" = "darwin" ]; then
    need_cmd codesign
    need_cmd spctl
  fi

  # Where the last install put it, so running this again updates that copy instead of leaving a
  # second one somewhere else on the PATH. INSTALL_DIR wins, for a person who is moving it.
  if [ -z "${INSTALL_DIR:-}" ] && [ -n "$INSTALLED_BY" ] && [ -r "$INSTALLED_BY" ]; then
    recorded="$(head -n 1 "$INSTALLED_BY" 2>/dev/null || true)"
    if [ -n "$recorded" ]; then
      INSTALL_DIR="$(dirname "$recorded")"
    fi
  fi
  INSTALL_DIR="${INSTALL_DIR:-$DEFAULT_INSTALL_DIR}"

  if [ -z "$TAG" ]; then
    TAG="$(curl -fsSL "$API_URL" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
    [ -n "$TAG" ] || fail "unable to resolve the latest release tag from $API_URL"
  fi

  BASE_URL="https://github.com/${REPO}/releases/download/${TAG}"
  TMP_DIR="$(mktemp -d)"
  trap 'rm -rf "$TMP_DIR"' EXIT

  BIN_PATH="${TMP_DIR}/${ASSET_NAME}"
  SHA_PATH="${TMP_DIR}/${ASSET_NAME}.sha256"

  echo "Downloading ${ASSET_NAME} ${TAG}..."
  curl -fsSL "${BASE_URL}/${ASSET_NAME}" -o "$BIN_PATH" ||
    fail "unable to download ${ASSET_NAME} for ${TAG}; check that this release exists for your platform"
  curl -fsSL "${BASE_URL}/${ASSET_NAME}.sha256" -o "$SHA_PATH"

  EXPECTED="$(tr -d '[:space:]' < "$SHA_PATH")"
  is_sha256 "$EXPECTED" || fail "malformed checksum for ${ASSET_NAME}"

  if command -v shasum >/dev/null 2>&1; then
    ACTUAL="$(shasum -a 256 "$BIN_PATH" | cut -d ' ' -f 1)"
  elif command -v sha256sum >/dev/null 2>&1; then
    ACTUAL="$(sha256sum "$BIN_PATH" | cut -d ' ' -f 1)"
  else
    fail "need shasum or sha256sum to verify the download"
  fi

  # Lowercased both sides, since the comparison is of two digests and not of two spellings.
  EXPECTED="$(printf '%s' "$EXPECTED" | tr 'A-F' 'a-f')"
  ACTUAL="$(printf '%s' "$ACTUAL" | tr 'A-F' 'a-f')"
  if [ "$ACTUAL" != "$EXPECTED" ]; then
    echo "error: checksum mismatch for ${ASSET_NAME}" >&2
    echo "expected: $EXPECTED" >&2
    echo "actual:   $ACTUAL" >&2
    exit 1
  fi

  # Linux ships no code signature, unlike Darwin (notarized) and Windows (Authenticode), so its
  # checksum carries a detached GPG signature instead. With gpg here, a signature that is missing
  # is refused like one that is wrong, since deleting it is what somebody replacing the release
  # would do. Without gpg the check is skipped, and said to be.
  if [ "$OS_KEY" = "linux" ]; then
    if command -v gpg >/dev/null 2>&1; then
      ASC_PATH="${TMP_DIR}/${ASSET_NAME}.sha256.asc"
      PUBKEY_PATH="${TMP_DIR}/bravebot-release.asc"
      curl -fsSL "${BASE_URL}/${ASSET_NAME}.sha256.asc" -o "$ASC_PATH" ||
        fail "unable to download the checksum signature for ${ASSET_NAME}"
      printf '%s\n' "$RELEASE_PUBKEY" > "$PUBKEY_PATH"
      verify_checksum_signature "$ASC_PATH" "$SHA_PATH" "$PUBKEY_PATH" "$SIGNING_KEY_FINGERPRINT" ||
        fail "signature verification failed for ${ASSET_NAME}.sha256; refusing to install"
    else
      echo "note: gpg not found; skipping signature verification (the checksum above was still verified)."
    fi
  fi

  if [ "$OS_KEY" = "darwin" ]; then
    verify_code_signature "$BIN_PATH" ||
      fail "${ASSET_NAME} is not signed by Brave Software (team ${APPLE_TEAM_ID}), or Gatekeeper rejected it (checking notarization may need the network); refusing to install"
  fi

  chmod +x "$BIN_PATH"

  DEST_PATH="${INSTALL_DIR}/${BIN_NAME}"
  if [ ! -d "$INSTALL_DIR" ]; then
    mkdir -p "$INSTALL_DIR" 2>/dev/null || {
      need_cmd sudo
      sudo mkdir -p "$INSTALL_DIR"
    }
  fi
  if [ -w "$INSTALL_DIR" ]; then
    mv "$BIN_PATH" "$DEST_PATH"
  else
    need_cmd sudo
    sudo mv "$BIN_PATH" "$DEST_PATH"
  fi

  # What bravebot reads to know it can offer the command that updates this copy. A machine with no
  # HOME gets the binary and no notice about later releases, which is the same as no record at all.
  #
  # Created reachable only by this user, and narrowed where it is already there, for the reason the
  # program narrows it: this directory holds the prompt history, and at the umask that is readable
  # by every local account. Both the directory and the record are created with the mode they keep
  # rather than chmod'ed once they exist, since the other order leaves them open for the moment in
  # between; the chmod is what narrows a directory an earlier install left, and the record is
  # removed and written again rather than written over. A link is stepped over rather than
  # followed, as the program steps over one: chmod without -h resolves it on both platforms this
  # supports, so a linked directory would have an install setting the mode of wherever the link
  # leads, which is outside anything this was given.
  if [ -n "$STATE_DIR" ] && (umask 077 && mkdir -p "$STATE_DIR") 2>/dev/null; then
    if [ ! -L "$STATE_DIR" ]; then
      chmod 700 "$STATE_DIR" 2>/dev/null || true
    fi
    rm -f "$INSTALLED_BY" 2>/dev/null || true
    (umask 077 && printf '%s\n' "$DEST_PATH" > "$INSTALLED_BY") 2>/dev/null || true
  fi

  echo "Installed ${BIN_NAME} ${TAG} to ${DEST_PATH}"

  ON_PATH="$(command -v "$BIN_NAME" 2>/dev/null || true)"
  if [ -z "$ON_PATH" ]; then
    echo "note: ${INSTALL_DIR} is not on your PATH; add it to run ${BIN_NAME} by name."
  elif [ "$ON_PATH" != "$DEST_PATH" ]; then
    echo "note: ${ON_PATH} comes first on your PATH, so that is what \`${BIN_NAME}\` still runs."
  else
    echo "Run: ${BIN_NAME} --help"
  fi
}

# Sourced with BRAVEBOT_INSTALL_SH_TEST=1, this file installs nothing, so a test can call main itself.
if [ "${BRAVEBOT_INSTALL_SH_TEST:-0}" != "1" ]; then
  main "$@"
fi
