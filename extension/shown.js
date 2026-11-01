// The page's words, made safe to put in the confirmation window.
//
// A page chooses the name, the text, the address and the tag of an element, so
// it can put anything there: a line break to start a sentence of its own, a
// right-to-left override that draws the end of a word first, a character with no
// width, a space that is not a space. The window shows each of its words as text
// and marks every such character with its code point, in an element of its own
// that a character of the page cannot imitate, and shows no more of a word than
// its limit.

// The characters a person cannot see, or that change how the text around them is
// drawn: control and format characters (which include the bidirectional marks,
// embeddings, overrides and isolates), line and paragraph separators, private
// use and unpaired surrogates, every space but the plain one, the letters and
// the blank braille cell that draw nothing, the combining grapheme joiner, and
// the default ignorable characters. Those include the variation selectors and
// the Mongolian and Khmer marks that draw nothing, which are combining marks
// that a name in JavaScript may hold, so two names that look alike can be two
// variables. Every character that crates/core/src/invisible.rs lists is here.
const UNSEEN =
  /[\p{Cc}\p{Cf}\p{Zl}\p{Zp}\p{Co}\p{Cs}\p{Default_Ignorable_Code_Point}\u00a0\u1680\u2000-\u200a\u202f\u205f\u3000\u115f\u1160\u3164\uffa0\u2800\u034f]/u;

// A combining mark is drawn on the character before it, above or below it, and
// any number of them stack and draw over the text around the word. Ordinary text
// has a few after a letter (an accent, a vowel sign and a tone mark), so up to
// MARKS_PER_CHARACTER after one visible character are shown as they are. A mark
// beyond that, and a mark with no visible character before it (at the start of a
// word, after a space or after a character that was marked), is marked by its
// code point.
const COMBINING = /[\p{Mn}\p{Me}]/u;
export const MARKS_PER_CHARACTER = 3;

// What marks a character: its code point, as "U+202E".
export function codePointName(character) {
  const hex = character.codePointAt(0).toString(16).toUpperCase();
  return `U+${hex.padStart(4, "0")}`;
}

// The first `limit` characters of `text` as pieces to show: a string for a run
// of characters that can be seen, and `{ mark }` for one that cannot. `more` is
// how many characters were left out. A value that is not text has no pieces.
export function shownParts(text, limit = Infinity) {
  const pieces = [];
  if (typeof text !== "string") {
    return { pieces, more: 0 };
  }
  let run = "";
  let count = 0;
  let more = 0;
  // Whether the last character shown was a visible one for a mark to be drawn
  // on, and how many marks it has.
  let based = false;
  let marks = 0;
  const mark = (character) => {
    if (run !== "") {
      pieces.push(run);
      run = "";
    }
    pieces.push({ mark: codePointName(character) });
  };
  for (const character of text) {
    if (count >= limit) {
      more += 1;
      continue;
    }
    count += 1;
    if (UNSEEN.test(character)) {
      mark(character);
      based = false;
      marks = 0;
    } else if (COMBINING.test(character)) {
      if (based && marks < MARKS_PER_CHARACTER) {
        run += character;
        marks += 1;
      } else {
        mark(character);
      }
    } else {
      run += character;
      based = character !== " ";
      marks = 0;
    }
  }
  if (run !== "") {
    pieces.push(run);
  }
  return { pieces, more };
}
