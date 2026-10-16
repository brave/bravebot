import { displaySegments, displayString, isEscaped } from '../text/displayText'

describe('display escaping', () => {
  it('escapes exactly the characters the Unicode categories name and the other default-ignorable or blank-drawing ones, for every code point', () => {
    const reference = /^[\p{Cc}\p{Cf}\p{Zl}\p{Zp}\u034f\u115f\u1160\u17b4\u17b5\u180b-\u180d\u180f\u2800\u3164\ufe00-\ufe0f\uffa0\u{e0100}-\u{e01ef}\p{Cs}]$/u
    const mismatches: string[] = []
    for (let codePoint = 0; codePoint <= 0x10ffff; codePoint++) {
      if (codePoint >= 0xd800 && codePoint <= 0xdfff) {
        if (!isEscaped(codePoint, false)) mismatches.push(`lone surrogate ${codePoint.toString(16)}`)
        continue
      }
      const expected = reference.test(String.fromCodePoint(codePoint))
      if (isEscaped(codePoint, false) !== expected) mismatches.push(codePoint.toString(16))
    }
    expect(mismatches).toEqual([])
  })

  it('keeps a line break and a tab in multi-line text and escapes them in a single line', () => {
    expect(displayString('a\nb\tc', { multiline: true })).toBe('a\nb\tc')
    expect(displayString('a\nb\tc')).toBe('a[U+000A]b[U+0009]c')
  })

  it('writes out an override that would reverse the file name, and marks it as escaped', () => {
    const segments = displaySegments('invoice\u202Etxt.exe')
    expect(segments).toEqual([
      { text: 'invoice', escaped: false },
      { text: 'U+202E', escaped: true },
      { text: 'txt.exe', escaped: false },
    ])
  })

  it('escapes zero-width, bell, carriage return, tag characters and a lone surrogate', () => {
    const text = 'ad\u200Bmin\u0007\r\u{E0041}\uD800'
    const escaped = displaySegments(text).filter((segment) => segment.escaped).map((segment) => segment.text)
    expect(escaped).toEqual(['U+200B', 'U+0007', 'U+000D', 'U+E0041', 'U+D800'])
  })

  it('leaves right-to-left letters and astral emoji alone', () => {
    expect(displayString('שלום 123 \u{1F600}')).toBe('שלום 123 \u{1F600}')
  })

  it('does not let text that looks like an escape pass as one', () => {
    const [segment] = displaySegments('U+202E')
    expect(segment).toEqual({ text: 'U+202E', escaped: false })
  })
})
