// The files this extension shares with the second one, extension-trusted/, which
// has no build step and no package manager and so holds copies. shared-files.json
// lists them: `copied` are the files the second holds byte for byte, `own` are the
// files a copied script imports that each extension writes for itself, and
// `pending` are the copied files the second does not hold yet, none today. The
// comparison with the second is run against it and is itself tested against a
// directory made here.
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import * as errors from "../errors.js";
import * as input from "../input.js";
import * as tools from "../tools.js";
import * as words from "../words.js";
import * as secondWords from "../../extension-trusted/words.js";

const FIRST = fileURLToPath(new URL("../", import.meta.url));
const SECOND = fileURLToPath(new URL("../../extension-trusted/", import.meta.url));
const LIST = JSON.parse(readFileSync(join(FIRST, "shared-files.json"), "utf8"));

// A file of this directory by name: a script, a page or a stylesheet.
const NAME = /^[a-z][a-z-]*\.(js|html|css)$/;

// The files a script imports or re-exports from, and the ones a page loads, each
// as "./name" when it is a file of the directory, outside comment lines.
function referencesOf(name, text) {
  const code = text
    .split("\n")
    .filter((line) => !line.trim().startsWith("//"))
    .join("\n");
  const pattern = name.endsWith(".js")
    ? /\b(?:import|export)\b[^;]*?\bfrom\s*"([^"]+)"|\bimport\s*"([^"]+)"/g
    : /\b(?:src|href)="([^"]+)"/g;
  return [...code.matchAll(pattern)]
    .map((match) => match[1] ?? match[2])
    .map((reference) => (/[/:]/.test(reference) ? reference : `./${reference}`));
}

// What is wrong with the copies in `second` of the files listed for `first`. A
// copied file that differs, one that is not there and is not pending, one that is
// pending and is there, a script of the second that imports a file it lacks, and
// a file that both hold with the same bytes and the list does not name, which no
// test would then hold equal.
function problemsWith(list, first, second) {
  const problems = [];
  for (const name of readdirSync(first).filter((file) => NAME.test(file) && existsSync(join(second, file)))) {
    if (
      !list.copied.includes(name) &&
      readFileSync(join(first, name)).equals(readFileSync(join(second, name)))
    ) {
      problems.push(`${name} is the same in both extensions and is not listed as copied`);
    }
  }
  for (const name of list.copied) {
    const pending = list.pending.includes(name);
    const copy = join(second, name);
    if (!existsSync(copy)) {
      if (!pending) {
        problems.push(`${name} is not in the second extension`);
      }
      continue;
    }
    if (pending) {
      problems.push(`${name} is in the second extension, so it is no longer pending`);
    }
    const mine = readFileSync(join(first, name));
    const theirs = readFileSync(copy);
    if (!mine.equals(theirs)) {
      problems.push(`${name} differs from the copy in the second extension`);
      continue;
    }
    for (const reference of referencesOf(name, theirs.toString("utf8"))) {
      if (reference.startsWith("./") && !existsSync(join(second, reference))) {
        problems.push(`${name} imports ${reference}, which the second extension lacks`);
      }
    }
  }
  return problems;
}

test("the list of shared files names each file of the extension once, and a pending file is one that is copied", () => {
  assert.deepEqual(Object.keys(LIST).sort(), ["copied", "own", "pending"]);
  for (const [which, names] of Object.entries(LIST)) {
    assert.ok(Array.isArray(names), which);
    assert.deepEqual(names, [...new Set(names)].sort(), `${which} is not sorted and unique`);
    for (const name of names) {
      assert.match(name, NAME, `${name} is not the name of a file of this directory`);
    }
  }
  assert.ok(LIST.copied.length > 0);
  for (const name of [...LIST.copied, ...LIST.own]) {
    assert.ok(existsSync(join(FIRST, name)), `${name} is not in the extension`);
  }
  assert.deepEqual(
    LIST.copied.filter((name) => LIST.own.includes(name)),
    [],
    "a file is both copied and written for each extension",
  );
  for (const name of LIST.pending) {
    assert.ok(LIST.copied.includes(name), `${name} is pending and is not copied`);
  }
});

test("a shared file imports and loads only shared files and the files each extension writes for itself", () => {
  const known = new Set([...LIST.copied, ...LIST.own]);
  let checked = 0;
  for (const name of LIST.copied.filter((file) => !file.endsWith(".css"))) {
    for (const reference of referencesOf(name, readFileSync(join(FIRST, name), "utf8"))) {
      assert.ok(
        reference.startsWith("./") && known.has(reference.slice(2)),
        `${name} refers to ${reference}`,
      );
      checked += 1;
    }
  }
  assert.ok(checked >= 6, `only ${checked} references were read`);
  for (const name of LIST.own) {
    assert.ok(
      LIST.copied.some((file) =>
        referencesOf(file, readFileSync(join(FIRST, file), "utf8")).includes(`./${name}`),
      ),
      `${name} is written for each extension and no copied file refers to it`,
    );
  }
});

test("input.js and tools.js still export what moved out of them, the same values", () => {
  const moved = Object.keys(words);
  assert.ok(moved.length >= 9, `only ${moved.length} names are exported by words.js`);
  for (const name of moved) {
    assert.equal(input[name], words[name], `input.js does not export ${name} of words.js`);
  }
  assert.equal(tools.Refusal, errors.Refusal);
  assert.equal(tools.ToolError, errors.ToolError);
  assert.ok(new tools.Refusal("x") instanceof errors.ToolError);
});

test("the comparison with the second extension finds a copy that differs, one that is missing, one that is pending and there, and a file a copy imports that is not", () => {
  // The list as it was while only words.js was copied, so every other file is pending.
  const early = { ...LIST, pending: LIST.copied.filter((name) => name !== "words.js") };
  const second = mkdtempSync(join(tmpdir(), "bravebot-shared-"));
  try {
    const copyAll = () => {
      for (const name of early.copied.filter((file) => !early.pending.includes(file))) {
        writeFileSync(join(second, name), readFileSync(join(FIRST, name)));
      }
    };
    copyAll();
    assert.deepEqual(problemsWith(early, FIRST, second), []);

    const copy = join(second, "words.js");
    writeFileSync(copy, `${readFileSync(copy, "utf8")}\n`);
    assert.deepEqual(problemsWith(early, FIRST, second), [
      "words.js differs from the copy in the second extension",
    ]);
    copyAll();

    rmSync(copy);
    assert.deepEqual(problemsWith(early, FIRST, second), [
      "words.js is not in the second extension",
    ]);
    copyAll();

    const pending = early.pending[0];
    writeFileSync(join(second, pending), readFileSync(join(FIRST, pending)));
    assert.deepEqual(problemsWith(early, FIRST, second), [
      `${pending} is in the second extension, so it is no longer pending`,
    ]);

    const all = { ...early, pending: [] };
    assert.deepEqual(
      problemsWith(all, FIRST, second),
      early.pending.slice(1).map((name) => `${name} is not in the second extension`),
    );

    for (const name of early.pending) {
      writeFileSync(join(second, name), readFileSync(join(FIRST, name)));
    }
    assert.deepEqual(problemsWith(all, FIRST, second), [
      "confirm.js imports ./actions.js, which the second extension lacks",
    ]);
    writeFileSync(join(second, "actions.js"), "export const ACTIONS = {};\n");
    assert.deepEqual(problemsWith(all, FIRST, second), []);

    // A file both hold with the same bytes has to be on the list.
    writeFileSync(join(second, "tools.js"), readFileSync(join(FIRST, "tools.js")));
    assert.deepEqual(problemsWith(all, FIRST, second), [
      "tools.js is the same in both extensions and is not listed as copied",
    ]);
    const unlisted = { ...all, copied: all.copied.filter((name) => name !== "confirmation.js") };
    writeFileSync(join(second, "tools.js"), "export const TOOLS = {};\n");
    assert.deepEqual(problemsWith(unlisted, FIRST, second), [
      "confirmation.js is the same in both extensions and is not listed as copied",
    ]);
  } finally {
    rmSync(second, { recursive: true, force: true });
  }
});

test("the shared files of the second extension are byte for byte those of this one", () => {
  assert.ok(existsSync(SECOND), "the second extension is not there");
  assert.deepEqual(problemsWith(LIST, FIRST, SECOND), []);
});

test("the limits of the page's words that the window shows, and the cap they are read up to, are the numbers BROWSER-34 states, in both extensions", () => {
  for (const [which, limits] of [["the BraveBot extension", words], ["the second extension", secondWords]]) {
    assert.deepEqual(
      {
        NAME_LIMIT: limits.NAME_LIMIT,
        TYPED_TEXT_LIMIT: limits.TYPED_TEXT_LIMIT,
        EXCERPT_LIMIT: limits.EXCERPT_LIMIT,
        TARGET_LIMIT: limits.TARGET_LIMIT,
        URL_LIMIT: limits.URL_LIMIT,
        SHOWN_CAP: limits.SHOWN_CAP,
      },
      {
        NAME_LIMIT: 200,
        TYPED_TEXT_LIMIT: 2_000,
        EXCERPT_LIMIT: 80,
        TARGET_LIMIT: 200,
        URL_LIMIT: 300,
        SHOWN_CAP: 2_000,
      },
      which,
    );
  }
});
