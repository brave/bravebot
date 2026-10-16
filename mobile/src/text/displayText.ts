// Placeholder for the shared Rust display helper (open-questions.md, Q2). Display only: nothing
// here decides what an approval or a turn may do. The ranges are checked against Unicode general
// categories Cf, Zl and Zp in src/__tests__/displayText.test.ts, and avoid \p{...} regexes so they do not
// depend on the JavaScript engine's Unicode tables.

export interface TextSegment {
  text: string
  escaped: boolean
}

// Cf, Zl and Zp beyond C1, plus the other characters that draw as nothing or as a blank (U+034F, the
// Hangul and Khmer fillers, the Mongolian and standard variation selectors, blank braille), as inclusive ranges.
// The joiner (U+200D) and the variation selectors (U+FE00-FE0F) are escaped even inside an emoji sequence, so such a
// sequence shows them written out. They can also hide text, and a person approving something needs to see them.
const HIDDEN: readonly (readonly [number, number])[] = [
  [0x00ad, 0x00ad], [0x034f, 0x034f], [0x0600, 0x0605], [0x061c, 0x061c], [0x06dd, 0x06dd], [0x070f, 0x070f],
  [0x0890, 0x0891], [0x08e2, 0x08e2], [0x115f, 0x1160], [0x17b4, 0x17b5], [0x180b, 0x180f], [0x200b, 0x200f], [0x2028, 0x202e],
  [0x2060, 0x2064], [0x2066, 0x206f], [0x2800, 0x2800], [0x3164, 0x3164], [0xfe00, 0xfe0f], [0xfeff, 0xfeff],
  [0xffa0, 0xffa0], [0xfff9, 0xfffb], [0x110bd, 0x110bd], [0x110cd, 0x110cd],
  [0x13430, 0x1343f], [0x1bca0, 0x1bca3], [0x1d173, 0x1d17a], [0xe0001, 0xe0001], [0xe0020, 0xe007f],
  [0xe0100, 0xe01ef],
]

function inRanges(codePoint: number): boolean {
  let low = 0
  let high = HIDDEN.length - 1
  while (low <= high) {
    const mid = (low + high) >> 1
    const range = HIDDEN[mid]!
    if (codePoint < range[0]) high = mid - 1
    else if (codePoint > range[1]) low = mid + 1
    else return true
  }
  return false
}

/** Whether a code point is written out instead of drawn. A line break and a tab are kept when `multiline`. */
export function isEscaped(codePoint: number, multiline: boolean): boolean {
  if (multiline && (codePoint === 0x0a || codePoint === 0x09)) return false
  if (codePoint <= 0x1f || (codePoint >= 0x7f && codePoint <= 0x9f)) return true
  if (codePoint >= 0xd800 && codePoint <= 0xdfff) return true
  return inRanges(codePoint)
}

function label(codePoint: number): string {
  return `U+${codePoint.toString(16).toUpperCase().padStart(4, '0')}`
}

/**
 * The text as runs: drawn text, and written-out code points for every character that could hide,
 * reorder or forge what is shown. The caller styles escaped runs itself, so content cannot imitate one.
 */
export function displaySegments(text: string, options: { multiline?: boolean } = {}): TextSegment[] {
  const multiline = options.multiline === true
  const segments: TextSegment[] = []
  let plain = ''
  const flush = (): void => {
    if (plain !== '') segments.push({ text: plain, escaped: false })
    plain = ''
  }
  for (let at = 0; at < text.length; ) {
    const codePoint = text.codePointAt(at)!
    const width = codePoint > 0xffff ? 2 : 1
    if (isEscaped(codePoint, multiline)) {
      flush()
      segments.push({ text: label(codePoint), escaped: true })
    } else {
      plain += text.slice(at, at + width)
    }
    at += width
  }
  flush()
  return segments
}

/** The text with escapes written inline, for places that take a string, such as an accessibility label. */
export function displayString(text: string, options: { multiline?: boolean } = {}): string {
  return displaySegments(text, options)
    .map((segment) => (segment.escaped ? `[${segment.text}]` : segment.text))
    .join('')
}
