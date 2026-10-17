#!/usr/bin/env node

// Thin launcher: forwards to the binary in the platform package npm installed beside this one.

const fs = require("node:fs");
const path = require("node:path");
const os = require("node:os");
const { spawnSync } = require("node:child_process");

// Named from the platform and architecture of the node running this, and from nothing else:
// not an argument, not a file, and not the environment. Which package is run decides which
// executable starts, so no value somebody supplies gets to pick it. The publish step names its
// packages the same way (npm/scripts/stage-platform-package.js), and npm/tests holds the two
// to each other.
const key = `${process.platform}-${process.arch}`;
const packageName = `@brave/bravebot-${key}`;
const binaryName = process.platform === "win32" ? "bravebot.exe" : "bravebot";

function findBinary() {
  let manifest;
  try {
    manifest = require.resolve(`${packageName}/package.json`);
  } catch {
    return { missing: "package" };
  }
  const binaryPath = path.join(path.dirname(manifest), "bin", binaryName);
  return fs.existsSync(binaryPath) ? { binaryPath } : { missing: "file" };
}

const found = findBinary();
if (!found.binaryPath) {
  const what =
    found.missing === "package"
      ? `its package, ${packageName}, is not installed`
      : `its package, ${packageName}, is installed but holds no binary`;
  console.error(
    `The bravebot binary for ${key} is missing: ${what}.\n` +
      "npm installs that package with @brave/bravebot, unless optional dependencies are omitted " +
      "(--omit=optional), the platform has no package, or the version is not published (an install " +
      "from a checkout or a git ref).\n" +
      "Reinstall without --omit=optional, or build bravebot from source."
  );
  process.exit(1);
}

// The binary is an ordinary executable in whatever directory npm unpacked it into, so this
// launcher is the only thing that knows the package it belongs to. It says so, and that is what
// lets bravebot offer the npm command when a newer release is out rather than one that would
// install a second copy somewhere else.
const result = spawnSync(found.binaryPath, process.argv.slice(2), {
  stdio: "inherit",
  env: { ...process.env, BRAVEBOT_INSTALLED_VIA: "npm" },
});

if (result.error) {
  console.error(`Failed to run bravebot: ${result.error.message}`);
  process.exit(1);
}

if (result.status !== null) {
  process.exit(result.status);
}

// Killed by a signal: report it the way a shell would, so `bravebot` behaves like the
// binary it wraps rather than collapsing every signal into a generic failure.
if (result.signal) {
  const signalNumber = os.constants.signals[result.signal];
  process.exit(typeof signalNumber === "number" ? 128 + signalNumber : 1);
}

process.exit(1);
