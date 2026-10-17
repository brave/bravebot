// What the published wrapper installs: no script, and exactly the six platform packages at its own
// version. Each of those is read off a file, so a manifest, a table and a workflow that disagree
// pass every other check and fail at somebody's install or at the release.
//
// Not published with the package. `make check-npm` runs it.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import test from "node:test";
import { fileURLToPath } from "node:url";

const read = (path) => readFileSync(fileURLToPath(new URL(path, import.meta.url)), "utf8");
const pkg = JSON.parse(read("../../package.json"));
const { PLATFORMS, packageName } = createRequire(import.meta.url)(
  fileURLToPath(new URL("../scripts/stage-platform-package.js", import.meta.url))
);

/**
 * npm runs these at install, and a skipped or unapproved one is what leaves a person with no
 * binary or with a failed install. The binary arrives as a package, so nothing is left to run.
 */
test("the wrapper runs no script when it is installed", () => {
  const scripts = Object.keys(pkg.scripts ?? {});

  assert.deepEqual(
    scripts.filter((name) => ["preinstall", "install", "postinstall", "prepare"].includes(name)),
    []
  );
});

test("the wrapper depends on nothing but its platform packages", () => {
  for (const kind of ["dependencies", "devDependencies", "peerDependencies"]) {
    assert.deepEqual(Object.keys(pkg[kind] ?? {}), [], kind);
  }
});

/**
 * A range lets npm take a binary from a release other than this wrapper's. A missing platform
 * leaves everyone on it with no binary, and a name that is not a platform installs a package
 * the launcher never runs.
 */
test("the platform packages are the six there are, each at this wrapper's exact version", () => {
  const expected = Object.fromEntries(
    ["darwin-arm64", "darwin-x64", "linux-arm64", "linux-x64", "win32-arm64", "win32-x64"].map(
      (key) => [`@brave/bravebot-${key}`, pkg.version]
    )
  );

  assert.deepEqual(pkg.optionalDependencies, expected);
});

test("the names the publish step builds are the names the wrapper depends on", () => {
  assert.deepEqual(
    Object.keys(PLATFORMS).map(packageName).sort(),
    Object.keys(pkg.optionalDependencies).sort()
  );
});

/**
 * The builder holds the signing key and the means to fetch a release, which a person installing
 * has no use for.
 */
test("the publish step's script does not ship in the package", () => {
  assert.deepEqual(
    pkg.files.filter((entry) => entry.startsWith("npm/scripts")),
    []
  );
  assert.ok(pkg.files.includes("npm/bin"), "the launcher ships");
});

/**
 * The workflow lists the platforms again as a matrix, because a matrix cannot be computed from a
 * script. A platform missing there is a package never published, and the wrapper that depends on
 * it installs without a binary. The runner has to be the system that platform's signature check
 * runs on: codesign on macOS, Authenticode on Windows, gpg on Linux.
 */
test("the publish workflow builds one package per platform, on that platform's own system", () => {
  const workflow = read("../../.github/workflows/publish-npm.yml");
  const matrix = [...workflow.matchAll(/^\s+- platform: (\S+)\n\s+runner: (\S+)$/gm)].map(
    (match) => [match[1], match[2]]
  );
  const system = { darwin: "macos-", linux: "ubuntu-", win32: "windows-" };

  assert.deepEqual(
    matrix.map(([key]) => key).sort(),
    Object.keys(PLATFORMS).sort()
  );
  for (const [key, runner] of matrix) {
    assert.ok(
      runner.startsWith(system[key.split("-")[0]]),
      `${key} is built on ${runner}, which cannot check its signature`
    );
  }
});
