#!/usr/bin/env node

// Creates the name of each platform package on the registry: `node bootstrap-platform-packages.js`
// publishes a placeholder for every @brave/bravebot-<platform> the registry has no package of, and
// `--check` only lists them and fails when there are any. npmjs.com attaches a trusted publisher to
// a package that exists, so the first version of each name cannot come from publish-npm.yml: OIDC
// has no record to match, and the registry answers the publish with a 404.
//
// The placeholder holds no binary and no script, so nothing is verified and nothing is built here.
// The first real version of each package is published by the workflow, with provenance, after the
// trusted publisher has been added. It is published under the `bootstrap` tag and the wrapper pins
// each platform package at its own exact version, so nobody installs it by following the wrapper.
//
// The publish is made with the maintainer's own `npm login` and one-time password. Nothing in this
// tree or in a workflow stores a credential for it.

const fs = require("node:fs");
const path = require("node:path");
const os = require("node:os");
const { spawnSync } = require("node:child_process");
const { PLATFORMS, packageName, manifest } = require("./stage-platform-package.js");

// A prerelease of a version nothing is released at, so it cannot satisfy the wrapper's pin or be
// taken for a release.
const PLACEHOLDER_VERSION = "0.0.0-bootstrap";
const PLACEHOLDER_TAG = "bootstrap";

// The one place the registry answers "no such package". Anything else it says, an outage or a
// refused request included, does not show that the name is free.
const NOT_FOUND = "E404";

// The page on npmjs.com where a package's trusted publisher is added.
function accessUrl(name) {
  return `https://www.npmjs.com/package/${name}/access`;
}

// The manifest of a package that reserves the name. The identifying fields and `os` and `cpu` are
// the platform package's own, so the listing says what the package will become; what a release
// adds is a binary, and what is left out here is that binary and every way of running code.
function placeholderManifest(key, root) {
  const [platform, arch] = key.split("-");
  return {
    ...manifest(key, root),
    version: PLACEHOLDER_VERSION,
    description: `Reserves the name of the bravebot package for ${platform} on ${arch}. It holds no binary. Install @brave/bravebot instead.`,
    files: ["LICENSE"],
  };
}

// Runs npm and returns what it said. `interactive` hands npm the terminal, which is what lets it ask
// for the one-time password. It is the one part of this file a test replaces.
function runNpm(args, { interactive = false } = {}) {
  const result = spawnSync("npm", args, {
    encoding: "utf8",
    stdio: interactive ? "inherit" : ["ignore", "pipe", "pipe"],
  });
  if (result.error) {
    throw new Error(`npm could not be run: ${result.error.message}`);
  }
  return { status: result.status, output: `${result.stdout ?? ""}${result.stderr ?? ""}` };
}

function registryHolds(name, npm) {
  const result = npm(["view", name, "name"]);
  if (result.status === 0) {
    return true;
  }
  if (result.output.includes(NOT_FOUND)) {
    return false;
  }
  throw new Error(`the registry did not say whether ${name} exists:\n${result.output.trim()}`);
}

// The platform keys whose package the registry does not have, in the order the table lists them.
function missingKeys(npm) {
  return Object.keys(PLATFORMS).filter((key) => !registryHolds(packageName(key), npm));
}

// Fails naming each platform package that does not exist, for the step that is about to dispatch
// the publish workflow.
function check({ npm = runNpm } = {}) {
  const missing = missingKeys(npm).map(packageName);
  if (missing.length > 0) {
    throw new Error(
      `the registry has no package named ${missing.join(", ")}, so publish-npm.yml cannot publish it: run make npm-bootstrap, then add a trusted publisher to each`
    );
  }
}

function writePlaceholder({ key, root, directory }) {
  fs.mkdirSync(directory, { recursive: true });
  fs.copyFileSync(path.join(__dirname, "../../LICENSE"), path.join(directory, "LICENSE"));
  fs.writeFileSync(
    path.join(directory, "package.json"),
    `${JSON.stringify(placeholderManifest(key, root), null, 2)}\n`
  );
}

// Publishes a placeholder for each missing package and returns the names it published. Nothing is
// written or published when none is missing, and nothing when npm is not logged in.
function bootstrap({ root, directory, npm = runNpm, log = console.log }) {
  const keys = missingKeys(npm);
  if (keys.length === 0) {
    log("every platform package is already on the registry");
    return [];
  }
  if (npm(["whoami"]).status !== 0) {
    throw new Error("npm is not logged in; run npm login as a maintainer who can publish @brave");
  }
  const published = [];
  for (const key of keys) {
    const name = packageName(key);
    const staged = path.join(directory, key);
    writePlaceholder({ key, root, directory: staged });
    const result = npm(
      ["publish", staged, "--access", "public", "--tag", PLACEHOLDER_TAG, "--ignore-scripts"],
      { interactive: true }
    );
    if (result.status !== 0) {
      throw new Error(`npm publish of ${name} failed`);
    }
    published.push(name);
    log(`published ${name}@${PLACEHOLDER_VERSION}`);
  }
  log("");
  log("Add a trusted publisher to each: GitHub Actions, repository brave/bravebot,");
  log("workflow publish-npm.yml, environment npm-publish.");
  for (const name of published) {
    log(`  ${accessUrl(name)}`);
  }
  return published;
}

function main(argv) {
  const root = require(path.join(__dirname, "../../package.json"));
  if (argv.length === 1 && argv[0] === "--check") {
    check();
    return;
  }
  if (argv.length !== 0) {
    console.error("usage: bootstrap-platform-packages.js [--check]");
    process.exit(2);
  }
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "bravebot-bootstrap-"));
  try {
    bootstrap({ root, directory });
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
}

// Importable, so what is published and what is refused can be checked against a fake npm: the
// work runs only when node was pointed at this file.
if (require.main === module) {
  try {
    main(process.argv.slice(2));
  } catch (error) {
    console.error(`error: ${error.message}`);
    process.exit(1);
  }
}

module.exports = {
  PLACEHOLDER_VERSION,
  PLACEHOLDER_TAG,
  accessUrl,
  placeholderManifest,
  registryHolds,
  missingKeys,
  check,
  bootstrap,
};
