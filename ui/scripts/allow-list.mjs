// The methods the window may ask the agent for, read out of `src/main/index.ts`.
//
// The main process holds them as one literal, `const ALLOWED = new Set([...])`, with `//` comments
// between the names. A comment can hold commas, apostrophes and quoted words of its own, so the
// comments are taken off before the quoted names are matched. Counting commas, or matching quotes in
// the text as it stands, reads a different list from the one the set holds at run time.
//
// `scripts/allow-list.test.mjs`, `scripts/session-directory.test.mjs` and `scripts/drive-menu.mjs`
// read the literal through this function, so that all three mean the same list by it. Seven other
// tests (`bot-store`, `bug-report`, `key-entry`, `sign-in`, `fetch-card`, `host-card` and
// `run-record`) match text inside the literal with their own checks, which do not take comments off.
// Every reader cuts the literal at its first closing bracket: this function and four of those tests
// at the first `]`, the other three at the first `])`. A comment inside the literal holds neither.

/** The method names in the literal, in the order written, or undefined when the source has no literal. */
export function allowedMethods(index) {
  const text = /const ALLOWED = new Set\(\[([^\]]*)\]\)/.exec(index)?.[1]
  if (text === undefined) return undefined
  return [...text.replace(/\/\/[^\n]*/g, '').matchAll(/'([^']+)'/g)].map((each) => each[1])
}
