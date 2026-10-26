// What the script that creates the platform packages' names publishes, and when it publishes
// nothing, checked against a fake npm so no registry is reached. A publish is not undone, so the
// cases that must not publish are answered here rather than at a maintainer's terminal.
//
// Not published with the package. `make check-npm` runs it.

import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const read = (path) => readFileSync(fileURLToPath(new URL(path, import.meta.url)), "utf8");
const require = createRequire(import.meta.url);
const scriptPath = fileURLToPath(new URL("../scripts/bootstrap-platform-packages.js", import.meta.url));
const { PLATFORMS, packageName } = require("../scripts/stage-platform-package.js");
const {
  PLACEHOLDER_VERSION,
  PLACEHOLDER_TAG,
  accessUrl,
  bootstrap,
  check,
  registryHolds,
} = require(scriptPath);

// A version no release has, so a placeholder that took its version from the root manifest cannot
// agree with it by accident.
const ROOT = {
  version: "9.8.7",
  license: "MPL-2.0",
  repository: { type: "git", url: "git+https://github.com/brave/bravebot.git" },
  bugs: { url: "https://github.com/brave/bravebot/issues" },
  homepage: "https://github.com/brave/bravebot#readme",
  publishConfig: { access: "public" },
};

const NAMES = Object.keys(PLATFORMS).map(packageName);

const NOT_FOUND = (name) =>
  `npm error code E404\nnpm error 404 Not Found - GET https://registry.npmjs.org/${name} - Not found\n`;

/**
 * A stand-in for npm. `existing` is the names the registry holds, `loggedIn` is whether `whoami`
 * answers, `unreachable` makes `view` fail with something that is not a 404, and `refuse` is the
 * names whose publish fails. Each publish records the manifest and the files it was handed, read
 * while the directory still exists.
 */
function fakeNpm({ existing = [], loggedIn = true, unreachable = false, refuse = [] } = {}) {
  const calls = [];
  const published = [];
  const npm = (args, options = {}) => {
    calls.push({ args, interactive: options.interactive === true });
    if (args[0] === "view") {
      if (unreachable) {
        return { status: 1, output: "npm error code E503\nnpm error 503 Service Unavailable\n" };
      }
      return existing.includes(args[1])
        ? { status: 0, output: `${args[1]}\n` }
        : { status: 1, output: NOT_FOUND(args[1]) };
    }
    if (args[0] === "whoami") {
      return loggedIn ? { status: 0, output: "maintainer\n" } : { status: 1, output: "ENEEDAUTH" };
    }
    assert.equal(args[0], "publish", `npm was asked to run ${args[0]}`);
    const manifest = JSON.parse(readFileSync(join(args[1], "package.json"), "utf8"));
    published.push({ args, manifest, files: readdirSync(args[1]).sort() });
    return refuse.includes(manifest.name)
      ? { status: 1, output: "" }
      : { status: 0, output: "" };
  };
  return { npm, calls, published };
}

function scratch(run) {
  const directory = mkdtempSync(join(tmpdir(), "bravebot-bootstrap-test-"));
  try {
    return run(directory);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

function run({ directory, ...fake }) {
  const lines = [];
  const outcome = fakeNpm(fake);
  const result = bootstrap({
    root: ROOT,
    directory,
    npm: outcome.npm,
    log: (line) => lines.push(line),
  });
  return { ...outcome, result, lines };
}

/**
 * A name the registry already holds has versions that are released, and a publish there fails or
 * adds a version nobody asked for. Only the names it has no package of are published.
 */
test("a placeholder is published for each package the registry lacks and for none it holds", () =>
  scratch((directory) => {
    const held = [NAMES[0], NAMES[3]];
    const { published, result } = run({ directory, existing: held });

    const expected = NAMES.filter((name) => !held.includes(name));
    assert.deepEqual(result, expected);
    assert.deepEqual(published.map((entry) => entry.manifest.name), expected);
  }));

/**
 * A package that carries a binary or runs a script is published with no provenance and no second
 * person's approval, which is what the workflow exists to prevent. The placeholder is a manifest
 * and a licence.
 */
test("a placeholder holds no binary, no script and no dependency", () =>
  scratch((directory) => {
    const { published } = run({ directory });

    assert.equal(published.length, NAMES.length);
    for (const { manifest, files } of published) {
      assert.deepEqual(files, ["LICENSE", "package.json"], manifest.name);
      for (const field of ["scripts", "bin", "main", "exports", "dependencies", "optionalDependencies", "peerDependencies", "devDependencies"]) {
        assert.equal(manifest[field], undefined, `${manifest.name} declares ${field}`);
      }
      assert.deepEqual(manifest.files, ["LICENSE"], manifest.name);
    }
  }));

/**
 * The wrapper pins each platform package at its own version. A placeholder at that version, or at
 * a version a range of it matches, would be installed in place of a binary.
 */
test("the placeholder is a prerelease that is not the version of any release", () =>
  scratch((directory) => {
    const { published } = run({ directory });
    const wrapper = JSON.parse(read("../../package.json"));

    assert.match(PLACEHOLDER_VERSION, /^0\.0\.0-[0-9A-Za-z.-]+$/);
    for (const { manifest } of published) {
      assert.equal(manifest.version, PLACEHOLDER_VERSION);
      assert.notEqual(manifest.version, ROOT.version);
      assert.notEqual(manifest.version, wrapper.version);
    }
  }));

/**
 * `latest` is what an install without a version takes, and `--provenance` fails outside GitHub
 * Actions. The publish is also the one that has to reach the terminal for the one-time password.
 */
test("the publish is made under the bootstrap tag, from the terminal, with no provenance and no script", () =>
  scratch((directory) => {
    const { published, calls } = run({ directory, existing: NAMES.slice(1) });

    assert.deepEqual(published.map((entry) => entry.args), [
      [
        "publish",
        join(directory, "darwin-arm64"),
        "--access",
        "public",
        "--tag",
        PLACEHOLDER_TAG,
        "--ignore-scripts",
      ],
    ]);
    assert.notEqual(PLACEHOLDER_TAG, "latest");
    assert.deepEqual(
      calls.filter((call) => call.args[0] === "publish").map((call) => call.interactive),
      [true]
    );
  }));

test("each platform's placeholder is matched to that platform's machines", () =>
  scratch((directory) => {
    const { published } = run({ directory });

    for (const { manifest } of published) {
      const key = manifest.name.replace("@brave/bravebot-", "");
      const [platform, arch] = key.split("-");
      assert.deepEqual(manifest.os, [platform], manifest.name);
      assert.deepEqual(manifest.cpu, [arch], manifest.name);
    }
  }));

test("nothing is published or asked of npm beyond the registry when every package exists", () =>
  scratch((directory) => {
    const { calls, published, result, lines } = run({ directory, existing: NAMES });

    assert.deepEqual(result, []);
    assert.deepEqual(published, []);
    assert.deepEqual(
      calls.map((call) => call.args[0]),
      NAMES.map(() => "view")
    );
    assert.deepEqual(lines, ["every platform package is already on the registry"]);
    assert.deepEqual(readdirSync(directory), []);
  }));

/**
 * A publish that stops partway for want of a login leaves some names created and others not,
 * and a second run has to be told which. Nothing is written or published without one.
 */
test("without a login nothing is written or published", () =>
  scratch((directory) => {
    assert.throws(() => run({ directory, loggedIn: false }), /not logged in/);

    assert.deepEqual(readdirSync(directory), []);
  }));

test("a failed publish stops the run, so the packages after it are not attempted", () =>
  scratch((directory) => {
    const outcome = fakeNpm({ refuse: [NAMES[1]] });

    assert.throws(
      () => bootstrap({ root: ROOT, directory, npm: outcome.npm, log: () => {} }),
      /npm publish of @brave\/bravebot-darwin-x64 failed/
    );
    assert.deepEqual(outcome.published.map((entry) => entry.manifest.name), NAMES.slice(0, 2));
  }));

/**
 * An outage or a refused request is not an answer about the name. Treating it as "absent" would
 * try to publish over names that exist, and treating a 404 as an error would never publish.
 */
test("only a 404 says a package is absent", () => {
  assert.equal(registryHolds("@brave/x", fakeNpm({ existing: ["@brave/x"] }).npm), true);
  assert.equal(registryHolds("@brave/x", fakeNpm().npm), false);
  assert.throws(
    () => registryHolds("@brave/x", fakeNpm({ unreachable: true }).npm),
    /did not say whether @brave\/x exists/
  );
});

test("an unreachable registry publishes nothing", () =>
  scratch((directory) => {
    assert.throws(() => run({ directory, unreachable: true }), /did not say whether/);

    assert.deepEqual(readdirSync(directory), []);
  }));

test("the instructions name the page where each published package takes a trusted publisher", () =>
  scratch((directory) => {
    const { lines } = run({ directory, existing: NAMES.slice(2) });
    const text = lines.join("\n");

    assert.match(text, /repository brave\/bravebot/);
    assert.match(text, /workflow publish-npm\.yml/);
    assert.match(text, /environment npm-publish/);
    for (const name of NAMES.slice(0, 2)) {
      assert.ok(text.includes(accessUrl(name)), name);
    }
    for (const name of NAMES.slice(2)) {
      assert.ok(!text.includes(accessUrl(name)), `${name} was not published`);
    }
  }));

/** The check is what the dispatch step runs, so it fails on the first absence and names them all. */
test("the check names every package the registry lacks, and passes when there is none", () => {
  const held = [NAMES[0], NAMES[5]];

  assert.throws(
    () => check({ npm: fakeNpm({ existing: held }).npm }),
    (error) =>
      NAMES.filter((name) => !held.includes(name)).every((name) => error.message.includes(name)) &&
      held.every((name) => !error.message.includes(name)) &&
      /make npm-bootstrap/.test(error.message)
  );
  assert.doesNotThrow(() => check({ npm: fakeNpm({ existing: NAMES }).npm }));
});

test("the check publishes nothing", () => {
  const { calls } = (() => {
    const outcome = fakeNpm();
    assert.throws(() => check({ npm: outcome.npm }));
    return outcome;
  })();

  assert.deepEqual([...new Set(calls.map((call) => call.args[0]))], ["view"]);
});

/**
 * The dispatch is the step that costs a second person's approval and six runner jobs. A missing
 * package fails it after both, so the Makefile asks first.
 */
test("make publish-npm checks the registry before it dispatches the workflow", () => {
  const makefile = read("../../Makefile");
  const recipe = makefile.slice(makefile.indexOf("\npublish-npm:"), makefile.indexOf("\n.PHONY: package-manifests"));

  const checked = recipe.indexOf("bootstrap-platform-packages.js --check");
  const dispatched = recipe.indexOf("gh workflow run publish-npm.yml");
  assert.ok(checked !== -1, "publish-npm does not run the check");
  assert.ok(dispatched !== -1, "publish-npm does not dispatch");
  assert.ok(checked < dispatched, "the check runs after the dispatch");
});

/**
 * Anything read from the environment is a way for something outside the script to choose what is
 * published or where, so the script reads none.
 */
test("the script reads no environment value", () => {
  const code = readFileSync(scriptPath, "utf8").replace(/^\s*\/\/.*$/gm, "");
  const reads = [...code.matchAll(/process\.env\s*(\.\w+|\[[^\]]*\])?/g)].map(
    (match) => match[1] ?? "(the whole environment, destructured or passed on)"
  );

  assert.deepEqual(reads, []);
});

/**
 * Only the registry's own host is ever reached, and only by npm: a script that fetched or
 * published by another route would be outside the login this relies on.
 */
test("the script starts npm and nothing else, and publishes nowhere but the default registry", () => {
  const code = readFileSync(scriptPath, "utf8").replace(/^\s*\/\/.*$/gm, "");

  assert.deepEqual([...code.matchAll(/spawnSync\(\s*"([^"]+)"/g)].map((match) => match[1]), ["npm"]);
  assert.ok(!/--registry|https?:\/\/(?!www\.npmjs\.com\/package\/)/.test(code));
});
