// The characters the confirmation window marks by their code point instead of
// drawing, one at a time, in both copies of shown.js. The class is a list of
// categories and single characters, and removing any one of them has to fail here.
// The window also marks every character that the terminal's question marks as
// drawing nothing, which is the list that crates/core/tests/draws-as-nothing.json
// holds and that a test of crates/core/src/invisible.rs holds draws_as_nothing to.
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { shownParts as first } from "../shown.js";
import { shownParts as second } from "../../extension-trusted/shown.js";

// A character of each kind the window cannot let a page draw as itself, by code
// point.
const UNSEEN = [
  ["a control character", 0x0001],
  ["a delete", 0x007f],
  ["a format character, the right-to-left override", 0x202e],
  ["a format character, the zero width space", 0x200b],
  ["a line separator", 0x2028],
  ["a paragraph separator", 0x2029],
  ["a private use character", 0xe000],
  ["a private use character of a plane", 0xf0000],
  ["an unpaired high surrogate", 0xd800],
  ["an unpaired low surrogate", 0xdc00],
  ["a no-break space", 0x00a0],
  ["an ogham space mark", 0x1680],
  ["an en quad", 0x2000],
  ["a hair space", 0x200a],
  ["a narrow no-break space", 0x202f],
  ["a medium mathematical space", 0x205f],
  ["an ideographic space", 0x3000],
  ["a hangul choseong filler", 0x115f],
  ["a hangul jungseong filler", 0x1160],
  ["a hangul filler", 0x3164],
  ["a halfwidth hangul filler", 0xffa0],
  ["the blank braille cell", 0x2800],
  ["the combining grapheme joiner", 0x034f],
];

const nameOf = (codePoint) => `U+${codePoint.toString(16).toUpperCase().padStart(4, "0")}`;

for (const [which, shownParts] of [
  ["the BraveBot extension's", first],
  ["the second extension's", second],
]) {
  test(`${which} shown.js marks each kind of character a person cannot see, or that changes how the text around it is drawn, by its code point, and shows a plain space and a letter as they are`, () => {
    for (const [kind, codePoint] of UNSEEN) {
      const text = `a${String.fromCodePoint(codePoint)}b`;
      assert.deepEqual(shownParts(text).pieces, ["a", { mark: nameOf(codePoint) }, "b"], kind);
    }
    assert.deepEqual(shownParts("a b").pieces, ["a b"]);
    assert.deepEqual(shownParts("é").pieces, ["é"]);
  });
}

// The ranges of `draws_as_nothing` in crates/core/src/invisible.rs, first and last
// code point of each, and the characters left unmarked, from the file that a test
// of invisible.rs holds the function to for every code point. A change to the
// function is a change to the file, and so to what the window has to mark.
const LIST = JSON.parse(
  readFileSync(new URL("../../crates/core/tests/draws-as-nothing.json", import.meta.url), "utf8"),
);
const DRAWS_AS_NOTHING = LIST.draws_as_nothing;
const LEFT_UNMARKED = LIST.left_unmarked;

// The ranges among them whose characters are combining marks that draw nothing and
// that a name in JavaScript may hold: the variation selectors, the Mongolian free
// variation selectors and the Khmer inherent vowels. Code that holds one is a
// different name from the one it looks like.
const IN_A_NAME = [
  ["the variation selectors", 0xfe00, 0xfe0f],
  ["the variation selectors supplement", 0xe0100, 0xe01ef],
  ["the Mongolian free variation selectors", 0x180b, 0x180d],
  ["the Khmer inherent vowels", 0x17b4, 0x17b5],
];

for (const [which, shownParts] of [
  ["the BraveBot extension's", first],
  ["the second extension's", second],
]) {
  test(`${which} shown.js marks every character that the terminal's question marks as drawing nothing, after a letter and with a mark before it`, () => {
    let checked = 0;
    for (const [from, to] of DRAWS_AS_NOTHING) {
      for (let codePoint = from; codePoint <= to; codePoint += 1) {
        const hidden = String.fromCodePoint(codePoint);
        assert.deepEqual(
          shownParts(`a${hidden}b`).pieces,
          ["a", { mark: nameOf(codePoint) }, "b"],
          nameOf(codePoint),
        );
        // Not taken for one of the few combining marks that follow a letter.
        assert.deepEqual(
          shownParts(`a\u0301${hidden}`).pieces,
          ["a\u0301", { mark: nameOf(codePoint) }],
          nameOf(codePoint),
        );
        checked += 1;
      }
    }
    assert.ok(checked > 4_000, `${checked} characters`);
  });

  test(`${which} shown.js leaves the characters of the list's left_unmarked as they are, after a letter and with a mark before it: the object replacement character draws as a visible box`, () => {
    assert.ok(LEFT_UNMARKED.some(([from]) => from === 0xfffc), "U+FFFC is not in the list");
    for (const [from, to] of LEFT_UNMARKED) {
      for (let codePoint = from; codePoint <= to; codePoint += 1) {
        const drawn = String.fromCodePoint(codePoint);
        assert.deepEqual(shownParts(`a${drawn}b`).pieces, [`a${drawn}b`], nameOf(codePoint));
        assert.deepEqual(shownParts(`a\u0301${drawn}`).pieces, [`a\u0301${drawn}`], nameOf(codePoint));
      }
    }
  });

  test(`${which} shown.js marks the combining marks that draw nothing and that a name in JavaScript may hold, so code that holds one differs from the code it looks like`, () => {
    for (const [kind, from, to] of IN_A_NAME) {
      for (let codePoint = from; codePoint <= to; codePoint += 1) {
        const hidden = String.fromCodePoint(codePoint);
        assert.match(`a${hidden}`, /^\p{ID_Start}\p{ID_Continue}$/u, `${nameOf(codePoint)} of ${kind} is not in a name`);
      }
    }
    const code = "const price = 1; const pric\u{fe0f}e = 2; const p\u{e0100}rice = 3; p\u{180b}rice;";
    const marks = shownParts(code).pieces.filter((piece) => typeof piece === "object");
    assert.deepEqual(marks, [{ mark: "U+FE0F" }, { mark: "U+E0100" }, { mark: "U+180B" }]);
  });
}
