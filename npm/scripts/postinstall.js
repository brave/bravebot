#!/usr/bin/env node

// Downloads the release binary for this platform and verifies its checksum and, on Linux, its
// detached GPG signature.
//
// The checksum check is not optional: without it, a network-fetched executable would
// run on the strength of TLS alone, and a compromised or substituted release asset
// would be indistinguishable from a good one.

const fs = require("node:fs");
const path = require("node:path");
const os = require("node:os");
const https = require("node:https");
const crypto = require("node:crypto");
const { spawnSync } = require("node:child_process");

const SKIP_ENV = "BRAVEBOT_INSTALL_SKIP_DOWNLOAD";
// Composed from nothing: not an argument, not a file, and not the environment. The asset and
// its checksum are fetched from the same release, so an origin something outside this file
// could supply would move both halves of the comparison at once, and a substituted binary
// published beside its own true digest would pass the check and be written onto PATH.
// install.sh states the same repository the same way.
const REPO = "brave/bravebot";
const MAX_REDIRECTS = 5;
// The key a Linux checksum has to be signed by. This is the public half, embedded so it has no
// external dependency. The fingerprint ensures only this specific key is trusted.
const SIGNING_KEY_FINGERPRINT = "13F28F0405C49B0B232DBA1BC1E827646A2DE416";
const RELEASE_PUBKEY = `-----BEGIN PGP PUBLIC KEY BLOCK-----

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
-----END PGP PUBLIC KEY BLOCK-----`;

function releaseBaseUrl(tag) {
  return `https://github.com/${REPO}/releases/download/${tag}`;
}

async function main() {
  const pkg = require(path.join(__dirname, "../../package.json"));

  // Lets the package install in CI or a sandbox with no network, and during local
  // development where the binary is built rather than downloaded.
  if (process.env[SKIP_ENV] === "1") {
    console.log(`Skipping bravebot binary download because ${SKIP_ENV}=1`);
    return;
  }

  const target = resolveTarget(process.platform, process.arch);
  if (!target) {
    console.error(`Unsupported platform/arch: ${process.platform}/${process.arch}`);
    process.exit(1);
  }

  await install({
    target,
    tag: `v${pkg.version}`,
    destination: path.join(__dirname, "..", "bin", target.binaryName),
  });
}

async function install({
  target,
  tag,
  destination,
  fingerprint = SIGNING_KEY_FINGERPRINT,
  publicKey = RELEASE_PUBKEY,
}) {
  const baseUrl = releaseBaseUrl(tag);
  const shaBytes = await fetchToBuffer(`${baseUrl}/${target.asset}.sha256`);
  const expected = shaBytes.toString("utf8").trim();
  if (!/^[0-9a-f]{64}$/i.test(expected)) {
    throw new Error(`Malformed checksum for ${target.asset}`);
  }

  const bytes = await fetchToBuffer(`${baseUrl}/${target.asset}`);
  const actual = crypto.createHash("sha256").update(bytes).digest("hex");
  if (actual !== expected.toLowerCase()) {
    throw new Error(
      `Checksum mismatch for ${target.asset}: expected ${expected}, got ${actual}`
    );
  }

  // Linux ships no code signature, unlike Darwin (notarized) and Windows (Authenticode), so its
  // checksum carries a detached GPG signature instead. With gpg here, a signature that is missing
  // is refused like one that is wrong, since deleting it is what somebody replacing the release
  // would do. Without gpg the check is skipped, and said to be. Verified against the exact bytes
  // fetched for the checksum, not a reconstructed string, since that is what was actually signed.
  if (target.asset.includes("-linux-")) {
    if (hasGpg()) {
      const ascBytes = await fetchToBuffer(`${baseUrl}/${target.asset}.sha256.asc`);
      const publicKeyBytes = Buffer.from(publicKey, "utf8");
      if (!verifySignature(shaBytes, ascBytes, publicKeyBytes, fingerprint)) {
        throw new Error(
          `Signature verification failed for ${target.asset}.sha256; refusing to install`
        );
      }
    } else {
      console.log(
        "Note: gpg not found; skipping signature verification (the checksum above was still verified)."
      );
    }
  }

  fs.mkdirSync(path.dirname(destination), { recursive: true });
  fs.writeFileSync(destination, bytes, { mode: 0o755 });
  console.log(`Installed bravebot ${tag} (${target.asset})`);
}

function resolveTarget(platform, arch) {
  const resolved = resolveArch(platform, arch);
  const key = `${platform}-${resolved}`;
  const table = {
    "darwin-arm64": "bravebot-darwin-arm64",
    "darwin-x64": "bravebot-darwin-amd64",
    "linux-arm64": "bravebot-linux-arm64",
    "linux-x64": "bravebot-linux-amd64",
    "win32-arm64": "bravebot-windows-arm64.exe",
    "win32-x64": "bravebot-windows-amd64.exe",
  };
  const asset = table[key];
  if (!asset) {
    return null;
  }
  return {
    asset,
    binaryName: platform === "win32" ? "bravebot-bin.exe" : "bravebot-bin",
  };
}

// Under Rosetta, node reports x64 on an arm64 machine. Installing the x64 binary
// would work but run translated, so prefer the native one.
function resolveArch(platform, arch) {
  const override = process.env.BRAVEBOT_INSTALL_ARCH;
  if (override === "arm64" || override === "x64") {
    return override;
  }

  if (platform === "darwin" && arch === "x64") {
    const translated = sysctl("sysctl.proc_translated");
    const arm64Capable = sysctl("hw.optional.arm64");
    if (translated === "1" && arm64Capable === "1") {
      console.log("Detected Rosetta translation; installing the native arm64 binary.");
      return "arm64";
    }
  }

  return arch;
}

function sysctl(name) {
  const result = spawnSync("sysctl", ["-in", name], { encoding: "utf8" });
  return result.status === 0 ? (result.stdout || "").trim() : null;
}

function fetchToBuffer(url) {
  return new Promise((resolve, reject) => {
    get(url, 0, (error, response) => {
      if (error) {
        reject(error);
        return;
      }
      const chunks = [];
      response.on("data", (chunk) => chunks.push(chunk));
      response.on("end", () => resolve(Buffer.concat(chunks)));
      response.on("error", reject);
    });
  });
}

function get(url, redirects, callback) {
  if (redirects > MAX_REDIRECTS) {
    callback(new Error("Too many redirects"));
    return;
  }

  https
    .get(url, { headers: { "User-Agent": "bravebot-installer" } }, (response) => {
      const { statusCode, headers } = response;
      if (statusCode >= 300 && statusCode < 400 && headers.location) {
        response.resume();
        // Ordinary redirect resolution, not an origin check: release downloads redirect to
        // objects.githubusercontent.com, so pinning the origin here would break installs.
        // The chain starts at an https:// URL on the repository named above, which nothing
        // outside this file chooses, every hop is TLS-verified, the redirect count is capped
        // above, and the payload is checksum-verified before it is written.
        // nosemgrep: url-constructor-base
        get(new URL(headers.location, url).toString(), redirects + 1, callback);
        return;
      }
      if (statusCode !== 200) {
        response.resume();
        callback(new Error(`HTTP ${statusCode} for ${url}`));
        return;
      }
      callback(null, response);
    })
    .on("error", callback);
}

function hasGpg() {
  const result = spawnSync("gpg", ["--version"]);
  return result.status === 0;
}

// True only when the signature verifies and was made by the key with the given fingerprint. A
// good signature from any other key in the imported file is a refusal, which is what the
// fingerprint is for. The keyring is made for this call and removed after it, so the person's own
// is neither read nor added to; --homedir names it rather than GNUPGHOME, so this reads nothing
// from the environment.
function verifySignature(shaBytes, ascBytes, publicKeyBytes, fingerprint) {
  let gnupgHome;
  try {
    gnupgHome = fs.mkdtempSync(path.join(os.tmpdir(), "bravebot-gnupg-"));
  } catch {
    return false;
  }
  try {
    const shaPath = path.join(gnupgHome, "checksum.sha256");
    const ascPath = path.join(gnupgHome, "checksum.sha256.asc");
    fs.writeFileSync(shaPath, shaBytes);
    fs.writeFileSync(ascPath, ascBytes);

    const imported = spawnSync(
      "gpg",
      ["--batch", "--quiet", "--homedir", gnupgHome, "--import"],
      { input: publicKeyBytes }
    );
    if (imported.status !== 0) {
      return false;
    }
    const verified = spawnSync(
      "gpg",
      ["--batch", "--status-fd", "1", "--homedir", gnupgHome, "--verify", ascPath, shaPath],
      { encoding: "utf8" }
    );
    if (verified.status !== 0) {
      return false;
    }
    // The last field of VALIDSIG is the primary key's fingerprint, whichever subkey signed.
    return verified.stdout.split("\n").some((line) => {
      const fields = line.trim().split(" ");
      return (
        fields[0] === "[GNUPG:]" &&
        fields[1] === "VALIDSIG" &&
        fields[fields.length - 1] === fingerprint
      );
    });
  } finally {
    fs.rmSync(gnupgHome, { recursive: true, force: true });
  }
}

// Importable, so the origin can be checked without a network or an install: the download runs
// only when node was pointed at this file. install is exported too, so the signature check can
// be run against a release the test made rather than the one this repository publishes.
if (require.main === module) {
  main().catch((error) => {
    console.error(`Failed to install the bravebot binary: ${error.message}`);
    process.exit(1);
  });
}

module.exports = { releaseBaseUrl, install };
