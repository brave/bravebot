// What the script that builds a platform package downloads from, which platform's binary it
// packages, and what it writes, checked without a network. The origin is a value the script
// composes, so an environment that tries to move it is answered here rather than at somebody's
// publish.
//
// Not published with the package: package.json's `files` lists npm/bin, so this directory ships
// to nobody. `make check-npm` runs it, which is what the npm lockfile job runs.

import assert from "node:assert/strict";
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const scriptPath = fileURLToPath(new URL("../scripts/stage-platform-package.js", import.meta.url));

// Set before the script is loaded, because an origin read from the environment is read as the
// module evaluates. A test that sets it afterwards passes against that fault.
process.env.BRAVEBOT_REPO = "substitute/bravebot";

const { PLATFORMS, build, releaseBaseUrl } = createRequire(import.meta.url)(scriptPath);

// Line comments are dropped before the two tests below read the file, so a comment quoting a URL
// or an environment variable is prose rather than a failure.
const code = readFileSync(scriptPath, "utf8").replace(/^\s*\/\/.*$/gm, "");

/**
 * The asset and its checksum are fetched from one origin and compared against each other, so an
 * origin somebody running the script can supply is not a check on the payload at all: a
 * substituted release published beside its own true digest satisfies it, and the binary is
 * published to the registry under Brave's name.
 */
test("the release origin is this repository whatever the environment holds", () => {
  assert.equal(
    releaseBaseUrl("v1.2.3"),
    "https://github.com/brave/bravebot/releases/download/v1.2.3"
  );
});

/**
 * The test above holds one function, and a download that spelled its own URL out would leave that
 * function correct and unused. There is one URL in the script for the same reason there is one
 * repository: an asset and the checksum it is compared against have to come from the same place,
 * and that place has to be this one.
 */
test("the script composes every URL it fetches from that one origin", () => {
  const urls = [...code.matchAll(/https?:\/\/[^\s`"']+/g)].map((match) => match[0]);

  assert.deepEqual(urls, ["https://github.com/${REPO}/releases/download/${tag}"]);
});

/**
 * Any reach into the environment is a way for something outside the script to decide what gets
 * packaged, so the script reads none. A name here is a decision, not an oversight.
 */
test("the script reads no environment value", () => {
  const read = [...code.matchAll(/process\.env\s*(\.\w+|\[[^\]]*\])?/g)].map(
    (match) => match[1] ?? "(the whole environment, destructured or passed on)"
  );

  assert.deepEqual(read, []);
});

// Written out here and not read from the script, so a table that is wrong in the script is wrong
// against this one. The asset names are what Jenkins uploads (the Makefile names them), and `os`
// and `cpu` are node's values for the machine each binary is for, which are what npm matches a
// package against.
const EXPECTED = {
  "darwin-arm64": { asset: "bravebot-darwin-arm64", os: "darwin", cpu: "arm64", binary: "bravebot" },
  "darwin-x64": { asset: "bravebot-darwin-amd64", os: "darwin", cpu: "x64", binary: "bravebot" },
  "linux-arm64": { asset: "bravebot-linux-arm64", os: "linux", cpu: "arm64", binary: "bravebot" },
  "linux-x64": { asset: "bravebot-linux-amd64", os: "linux", cpu: "x64", binary: "bravebot" },
  "win32-arm64": {
    asset: "bravebot-windows-arm64.exe",
    os: "win32",
    cpu: "arm64",
    binary: "bravebot.exe",
  },
  "win32-x64": {
    asset: "bravebot-windows-amd64.exe",
    os: "win32",
    cpu: "x64",
    binary: "bravebot.exe",
  },
};

// A version no real release has, so a package that took its version from anywhere but the root
// manifest, or its tag from anywhere but that version, cannot agree by accident.
const ROOT = {
  version: "9.8.7",
  license: "MPL-2.0",
  repository: { type: "git", url: "git+https://github.com/brave/bravebot.git" },
  bugs: { url: "https://github.com/brave/bravebot/issues" },
  homepage: "https://github.com/brave/bravebot#readme",
  publishConfig: { access: "public" },
};

function scratch(run) {
  const directory = mkdtempSync(join(tmpdir(), "bravebot-stage-"));
  return run(directory).finally(() => rmSync(directory, { recursive: true, force: true }));
}

/** Stands in for the download: records what it was asked for and writes the "binary". */
function recordingFetch(asked) {
  return async ({ target, tag, destination }) => {
    asked.push({ asset: target.asset, tag });
    mkdirSync(join(destination, ".."), { recursive: true });
    writeFileSync(destination, "binary", { mode: 0o755 });
  };
}

test("the platforms packaged are the six there are binaries for, and no others", () => {
  assert.deepEqual(Object.keys(PLATFORMS).sort(), Object.keys(EXPECTED).sort());
});

/**
 * Each platform's package has to hold that platform's asset and be matched to that platform's
 * machines. An arm64 asset in the x64 package, or a package whose `cpu` says the other
 * architecture, installs a binary that does not run, or installs none, with nothing failing.
 */
for (const [key, expected] of Object.entries(EXPECTED)) {
  test(`${key} packages ${expected.asset} for ${expected.os} on ${expected.cpu}`, () =>
    scratch(async (directory) => {
      const asked = [];
      await build({ key, directory, root: ROOT, fetchBinary: recordingFetch(asked) });

      assert.deepEqual(asked, [{ asset: expected.asset, tag: "v9.8.7" }]);
      assert.deepEqual(JSON.parse(readFileSync(join(directory, "package.json"), "utf8")), {
        name: `@brave/bravebot-${key}`,
        version: "9.8.7",
        description: `The bravebot binary for ${expected.os} on ${expected.cpu}. Install @brave/bravebot, which depends on this package.`,
        license: "MPL-2.0",
        repository: ROOT.repository,
        bugs: ROOT.bugs,
        homepage: ROOT.homepage,
        os: [expected.os],
        cpu: [expected.cpu],
        files: ["bin", "LICENSE"],
        publishConfig: { access: "public" },
      });
      assert.deepEqual(readdirSync(join(directory, "bin")), [expected.binary]);
      assert.deepEqual(readdirSync(directory).sort(), ["LICENSE", "bin", "package.json"]);
    }));
}

/**
 * The manifest is what says the directory is a package, so a directory that holds one holds a
 * binary that was checked. A download that refuses has to leave no manifest to publish.
 */
test("a refused download leaves no manifest behind", () =>
  scratch(async (directory) => {
    await assert.rejects(
      build({
        key: "darwin-arm64",
        directory,
        root: ROOT,
        fetchBinary: async () => {
          throw new Error("Checksum mismatch");
        },
      }),
      /Checksum mismatch/
    );

    assert.deepEqual(readdirSync(directory), []);
  }));

/**
 * A key outside the table must not reach the download, and the names every object inherits are
 * not in the table either.
 */
for (const key of ["freebsd-x64", "linux-ia32", "", "constructor", "__proto__", "toString"]) {
  test(`no package is built for ${JSON.stringify(key)}`, () =>
    scratch(async (directory) => {
      const asked = [];
      await assert.rejects(
        build({ key, directory, root: ROOT, fetchBinary: recordingFetch(asked) }),
        /No package is built for/
      );

      assert.deepEqual(asked, []);
      assert.deepEqual(readdirSync(directory), []);
    }));
}
