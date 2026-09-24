// What the npm installer downloads from, checked without a network. The origin is a value the
// installer composes, so an environment that tries to move it is answered here rather than at
// somebody's install.
//
// Not published with the package: package.json's `files` lists npm/bin and npm/scripts, so this
// directory ships to nobody. `make check-npm` runs it, which is what the npm lockfile job runs.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import test from "node:test";
import { fileURLToPath } from "node:url";

const scriptPath = fileURLToPath(new URL("../scripts/postinstall.js", import.meta.url));

// Set before the installer is loaded, because an origin read from the environment is read as the
// module evaluates. A test that sets it afterwards passes against that fault.
process.env.BRAVEBOT_REPO = "substitute/bravebot";

const { releaseBaseUrl } = createRequire(import.meta.url)(scriptPath);

// Line comments are dropped before the two tests below read the file, so a comment quoting a URL
// or an environment variable is prose rather than a failure.
const code = readFileSync(scriptPath, "utf8").replace(/^\s*\/\/.*$/gm, "");

/**
 * The asset and its checksum are fetched from one origin and compared against each other, so an
 * origin somebody installing can supply is not a check on the payload at all: a substituted
 * release published beside its own true digest satisfies it, and the binary is written executable
 * and put on PATH.
 */
test("the release origin is this repository whatever the environment holds", () => {
  assert.equal(
    releaseBaseUrl("v1.2.3"),
    "https://github.com/brave/bravebot/releases/download/v1.2.3"
  );
});

/**
 * The test above holds one function, and a download that spelled its own URL out would leave that
 * function correct and unused. There is one URL in the installer for the same reason there is one
 * repository: an asset and the checksum it is compared against have to come from the same place,
 * and that place has to be this one.
 */
test("the installer composes every URL it fetches from that one origin", () => {
  const urls = [...code.matchAll(/https?:\/\/[^\s`"']+/g)].map((match) => match[0]);

  assert.deepEqual(urls, ["https://github.com/${REPO}/releases/download/${tag}"]);
});

/**
 * The two values below choose whether to download and which architecture to download, and neither
 * can change where the bytes come from. Any other reach into the environment is a way for
 * something outside the installer to decide what gets installed, which is what this rejects: a
 * third name here is a decision, not an oversight.
 */
test("the installer reads no environment value that could move the origin", () => {
  const read = [...code.matchAll(/process\.env\s*(\.\w+|\[[^\]]*\])?/g)].map(
    (match) => match[1] ?? "(the whole environment, destructured or passed on)"
  );

  assert.deepEqual([...new Set(read)].sort(), [".BRAVEBOT_INSTALL_ARCH", "[SKIP_ENV]"]);
});
