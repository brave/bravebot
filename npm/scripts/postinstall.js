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
const DEFAULT_REPO = "brave/bravebot";
const MAX_REDIRECTS = 5;
const PUBKEY_URL = "https://brave-browser-downloads.s3.brave.com/keys/bravebot-release.asc";
// The key a Linux checksum has to be signed by. The file at PUBKEY_URL is only where its public
// half is fetched from, so whoever controls that host can add a key to it but cannot become this one.
const SIGNING_KEY_FINGERPRINT = "13F28F0405C49B0B232DBA1BC1E827646A2DE416";

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

async function fetchToString(url) {
  return (await fetchToBuffer(url)).toString("utf8");
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
        // The chain starts at an https:// URL, every hop is TLS-verified, the redirect count
        // is capped above, and the payload is checksum-verified before it is written.
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
// is neither read nor added to.
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

    const env = { ...process.env, GNUPGHOME: gnupgHome };
    const imported = spawnSync("gpg", ["--batch", "--quiet", "--import"], {
      input: publicKeyBytes,
      env,
    });
    if (imported.status !== 0) {
      return false;
    }
    const verified = spawnSync(
      "gpg",
      ["--batch", "--status-fd", "1", "--verify", ascPath, shaPath],
      { env, encoding: "utf8" }
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

async function install(tag, target, baseUrl, destination, fingerprint = SIGNING_KEY_FINGERPRINT) {
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
      const publicKeyBytes = await fetchToBuffer(PUBKEY_URL);
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

function main() {
  const repo = process.env.BRAVEBOT_REPO || DEFAULT_REPO;
  const pkg = require(path.join(__dirname, "../../package.json"));
  const tag = `v${pkg.version}`;

  // Lets the package install in CI or a sandbox with no network, and during local
  // development where the binary is built rather than downloaded.
  if (process.env[SKIP_ENV] === "1") {
    console.log(`Skipping bravebot binary download because ${SKIP_ENV}=1`);
    return;
  }

  const target = resolveTarget(process.platform, process.arch);
  if (!target) {
    console.error(`Unsupported platform/arch: ${process.platform}/${process.arch}`);
    process.exitCode = 1;
    return;
  }

  const baseUrl = `https://github.com/${repo}/releases/download/${tag}`;
  const destination = path.join(__dirname, "..", "bin", target.binaryName);

  install(tag, target, baseUrl, destination).catch((error) => {
    console.error(`Failed to install the bravebot binary: ${error.message}`);
    process.exitCode = 1;
  });
}

// Required rather than run, this file installs nothing, so a test can call install() itself.
if (require.main === module) {
  main();
}

module.exports = { resolveTarget, resolveArch, verifySignature, install, main };
