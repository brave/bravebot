// What `bravebot` does once npm has installed it: which package's binary it runs, and what it says
// when that package is not there. Each test lays out a node_modules tree the way npm does and runs
// the real launcher in it, so the launcher is held to the layout the publish step produces and not
// to a layout the test invented.
//
// Not published with the package. `make check-npm` runs it.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  chmodSync,
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const launcherPath = fileURLToPath(new URL("../bin/bravebot.js", import.meta.url));
const stagePath = fileURLToPath(new URL("../scripts/stage-platform-package.js", import.meta.url));
const { PLATFORMS, build } = createRequire(import.meta.url)(stagePath);

// The binaries below are shell scripts, which Windows cannot execute as a program.
const posix = { skip: process.platform === "win32" && "the stand-in binary is a shell script" };

const key = `${process.platform}-${process.arch}`;
const other = Object.keys(PLATFORMS).find((candidate) => candidate !== key);

// Answers with what it was given and what the launcher told it, and fails on request.
const BINARY = `#!/bin/sh
printf 'package:%s\\n' "$BINARY_FROM"
printf 'arguments:%s\\n' "$*"
printf 'installed-via:%s\\n' "$BRAVEBOT_INSTALLED_VIA"
[ "$1" = fail ] && exit 7
exit 0
`;

function binaryFor(label) {
  return BINARY.replace('"$BINARY_FROM"', `"${label}"`);
}

/** A tree holding only the wrapper, with the real launcher where npm puts it. */
async function install(run) {
  const root = mkdtempSync(join(tmpdir(), "bravebot-launcher-"));
  const wrapper = join(root, "node_modules", "@brave", "bravebot");
  mkdirSync(join(wrapper, "npm", "bin"), { recursive: true });
  copyFileSync(launcherPath, join(wrapper, "npm", "bin", "bravebot.js"));
  try {
    return await run({ root, wrapper, launcher: join(wrapper, "npm", "bin", "bravebot.js") });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

/** Puts a platform package where npm would, with the binary at the path the launcher reads. */
function addPackage(directory, platform, label, binaryName = "bravebot") {
  const folder = join(directory, "@brave", `bravebot-${platform}`);
  mkdirSync(join(folder, "bin"), { recursive: true });
  writeFileSync(join(folder, "package.json"), JSON.stringify({ name: `@brave/bravebot-${platform}` }));
  writeFileSync(join(folder, "bin", binaryName), binaryFor(label));
  chmodSync(join(folder, "bin", binaryName), 0o755);
  return folder;
}

function run(launcher, ...args) {
  const result = spawnSync(process.execPath, [launcher, ...args], { encoding: "utf8" });
  return { status: result.status, stdout: result.stdout, stderr: result.stderr };
}

test("it runs the binary in the package for this platform, with the arguments it was given", posix, () =>
  install(({ root, launcher }) => {
    addPackage(join(root, "node_modules"), key, "this-platform");

    const result = run(launcher, "--version", "a b");

    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /^package:this-platform$/m);
    assert.match(result.stdout, /^arguments:--version a b$/m);
  }));

/**
 * The binary is told it was started by npm, which is what lets bravebot offer the npm command
 * when a newer release is out (UPDATE-2).
 */
test("it tells the binary that npm started it", posix, () =>
  install(({ root, launcher }) => {
    addPackage(join(root, "node_modules"), key, "this-platform");

    assert.match(run(launcher).stdout, /^installed-via:npm$/m);
  }));

test("it exits with the status the binary exits with", posix, () =>
  install(({ root, launcher }) => {
    addPackage(join(root, "node_modules"), key, "this-platform");

    assert.equal(run(launcher, "fail").status, 7);
  }));

/**
 * A wrapper installed beneath another package, or by a manager that does not hoist, has its own
 * node_modules, and the platform package is installed in that one.
 */
test("it finds the package when npm installed it beneath the wrapper", posix, () =>
  install(({ wrapper, launcher }) => {
    addPackage(join(wrapper, "node_modules"), key, "nested");

    assert.match(run(launcher).stdout, /^package:nested$/m);
  }));

/**
 * Every platform's package is installed by something at some point: a shared home directory, an
 * install made for another machine, a cache. Running whichever `@brave/bravebot-*` is found
 * starts a binary built for another system, or one that was never meant for this version.
 */
test("it does not run the package of another platform", posix, () =>
  install(({ root, launcher }) => {
    addPackage(join(root, "node_modules"), other, "other-platform");

    const result = run(launcher);

    assert.equal(result.status, 1);
    assert.doesNotMatch(result.stdout, /other-platform/);
    assert.match(result.stderr, new RegExp(`@brave/bravebot-${key}\\b`));
  }));

/**
 * `--omit=optional`, a platform with no package and a version that was never published all leave
 * the same tree: the wrapper and nothing beside it. The message has to say what is missing and
 * what to do, and has to name the package so a person can look for it.
 */
test("with no package installed it says which package is missing and why that happens", () =>
  install(({ launcher }) => {
    const result = run(launcher);

    assert.equal(result.status, 1);
    assert.equal(result.stdout, "");
    assert.match(result.stderr, new RegExp(`The bravebot binary for ${key} is missing`));
    assert.match(result.stderr, new RegExp(`@brave/bravebot-${key}, is not installed`));
    assert.match(result.stderr, /--omit=optional/);
    assert.match(result.stderr, /checkout/);
    assert.match(result.stderr, /build bravebot from source/);
  }));

test("a package that holds no binary is reported as that", () =>
  install(({ root, launcher }) => {
    const folder = addPackage(join(root, "node_modules"), key, "emptied");
    rmSync(join(folder, "bin"), { recursive: true });

    const result = run(launcher);

    assert.equal(result.status, 1);
    assert.match(result.stderr, /is installed but holds no binary/);
  }));

/**
 * What the launcher runs is chosen by the platform alone. Any reach into the environment beyond
 * handing it on to the binary is a way for a variable to choose it, and a new name here is a
 * decision, not an oversight.
 */
test("the launcher reads no environment value except to hand it on", () => {
  const code = readFileSync(launcherPath, "utf8").replace(/^\s*\/\/.*$/gm, "");

  const read = [...code.matchAll(/process\.env\s*(\.\w+|\[[^\]]*\])?/g)].map(
    (match) => match[1] ?? "(the whole environment, destructured or passed on)"
  );

  assert.deepEqual(read, ["(the whole environment, destructured or passed on)"]);
});

/**
 * The publish step writes the package and the launcher reads it, in two files that share nothing
 * but a convention, so a package built by the real builder has to be one the real launcher runs.
 */
test(
  "a package built by the publish step is one the launcher runs",
  { skip: posix.skip || (!PLATFORMS[key] && `no package is built for ${key}`) },
  () =>
    install(async ({ root, launcher }) => {
      const directory = join(root, "node_modules", "@brave", `bravebot-${key}`);
      await build({
        key,
        directory,
        root: { version: "9.8.7", license: "MPL-2.0" },
        fetchBinary: async ({ destination }) => {
          mkdirSync(dirname(destination), { recursive: true });
          writeFileSync(destination, binaryFor("built"), { mode: 0o755 });
        },
      });

      const result = run(launcher, "hello");

      assert.equal(result.status, 0, result.stderr);
      assert.match(result.stdout, /^package:built$/m);
      assert.match(result.stdout, /^arguments:hello$/m);
    })
);
