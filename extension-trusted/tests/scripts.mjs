// The code of the extension, for the tests that scan it: each script outside the
// tests, with its comment lines removed.

import { readdirSync, readFileSync } from "node:fs";

const DIRECTORY = new URL("..", import.meta.url);

// The scripts of the extension: file name and its lines of code.
export function scripts() {
  return readdirSync(DIRECTORY)
    .filter((name) => name.endsWith(".js"))
    .map((name) => ({
      name,
      code: readFileSync(new URL(name, DIRECTORY), "utf8")
        .split("\n")
        .filter((line) => !line.trimStart().startsWith("//")),
    }));
}

// How many times `pattern` matches in the code of `scripts`, by file.
export function occurrences(found, pattern) {
  const counts = {};
  for (const { name, code } of found) {
    const count = code.join("\n").match(new RegExp(pattern, "g"))?.length ?? 0;
    if (count > 0) {
      counts[name] = count;
    }
  }
  return counts;
}
