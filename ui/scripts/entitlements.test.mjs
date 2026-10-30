// The entitlements the release job signs the macOS app with.
//
// A hardened-runtime build that is granted more than it needs widens what a compromised renderer
// can do, and one that is granted less fails to start its renderer. The Electron processes' file
// holds the one entitlement V8 needs, and a second key is added only when a notarised build is shown
// to need it. The file for everything else grants nothing: `@electron/osx-sign` always passes an
// entitlements file, so signing an item with none means passing a file with no keys.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const BUILD = join(dirname(fileURLToPath(import.meta.url)), '..', 'build')
const JIT = [['com.apple.security.cs.allow-jit', true]]
const FILES = [
  ['entitlements.mac.plist', JIT, 'grants allow-jit and nothing else'],
  ['entitlements.mac.none.plist', [], 'grants nothing'],
]

// A property list whose top level is a dictionary of boolean entries, read strictly: any other
// element, a malformed entry or text outside the entries is a failure, so a file that a stricter
// reader would refuse does not pass here for want of a parser.
function booleanDictionary(text) {
  const frame = /^<\?xml version="1\.0" encoding="UTF-8"\?>\s*<!DOCTYPE plist [^>]*>\s*<plist version="1\.0">\s*<dict>([\s\S]*)<\/dict>\s*<\/plist>\s*$/
  const framed = frame.exec(text)
  assert.ok(framed, 'not a property list with a dictionary at the top')
  const entry = /<key>([^<]+)<\/key>\s*<(true|false)\/>/g
  const entries = [...framed[1].matchAll(entry)].map(([, key, value]) => [key, value === 'true'])
  assert.equal(framed[1].replace(entry, '').trim(), '', 'the dictionary holds something but boolean entries')
  return entries
}

for (const [name, entries, grants] of FILES) {
  test(`${name} parses and ${grants}`, () => {
    const text = readFileSync(join(BUILD, name), 'utf8')
    assert.deepEqual(booleanDictionary(text), entries)
  })
}

test('the reader refuses a file that is not a boolean dictionary', () => {
  const frame = (body) => `<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE plist PUBLIC "x" "y">\n<plist version="1.0">\n<dict>${body}</dict>\n</plist>\n`
  assert.deepEqual(booleanDictionary(frame('<key>a</key><true/>')), [['a', true]])
  assert.throws(() => booleanDictionary(frame('<key>a</key><string>x</string>')), /boolean entries/)
  assert.throws(() => booleanDictionary(frame('<key>a</key><true/><key>b</key>')), /boolean entries/)
  assert.throws(() => booleanDictionary('<dict></dict>'), /property list/)
})
