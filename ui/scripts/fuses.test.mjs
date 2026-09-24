// The fuses a release sets, written into the bytes Electron reads them from.
//
// Nothing about a running app says which fuses it has short of trying the door each one closes,
// so a write at the wrong offset, or one that quietly found nothing to write, packages an app
// indistinguishable from a fused one until somebody sets `ELECTRON_RUN_AS_NODE`.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { RELEASE_FUSES, SENTINEL, acceptsInspect, hasFuses, readFuses, setFuses } from './fuses.mjs'

// A binary with one wire per entry in `wires`, each `[version, fuses]`, between bytes that are not
// part of any wire, so a write that lands outside one shows up as a changed byte around it.
function binary(...wires) {
  const parts = [Buffer.from('before')]
  for (const [version, fuses] of wires) {
    parts.push(Buffer.from(SENTINEL), Buffer.from([version, fuses.length]), Buffer.from(fuses, 'latin1'), Buffer.from('between'))
  }
  return Buffer.concat(parts)
}

// Electron 44's wire as it ships: RunAsNode, the NODE_OPTIONS variable and the inspect arguments
// on, both asar checks off.
const SHIPPED = '101100011'

test('a release turns off running as Node, NODE_OPTIONS and --inspect, and turns on both asar checks', () => {
  const bytes = binary([1, SHIPPED])
  setFuses(bytes, RELEASE_FUSES)
  assert.deepEqual(readFuses(bytes), ['000011011'])
  assert.deepEqual(bytes, binary([1, '000011011']))
})

test('every wire in the binary is set, not only the first one found', () => {
  const bytes = binary([1, SHIPPED], [1, SHIPPED])
  setFuses(bytes, RELEASE_FUSES)
  assert.deepEqual(readFuses(bytes), ['000011011', '000011011'])
})

// Each of these is a binary the step cannot finish setting. Carrying on would package an app with
// some door still open and report success.
test('a binary the release fuses cannot all be set in is refused', () => {
  const refusals = [
    [Buffer.from('an executable with no wire in it'), 'no fuse wire: this is not an Electron binary'],
    [binary([2, SHIPPED]), 'fuse wire version 2, and this writes version 1'],
    [binary([1, '1011']), 'this Electron has no EnableEmbeddedAsarIntegrityValidation fuse'],
    [binary([1, 'r01100011']), 'this Electron has removed the RunAsNode fuse'],
    [Buffer.concat([Buffer.from(SENTINEL), Buffer.from([1, 9]), Buffer.from('101')]), 'fuse wire cut off: the binary ends inside it'],
    [Buffer.concat([Buffer.from(SENTINEL), Buffer.from([1])]), 'fuse wire cut off: the binary ends inside it'],
  ]
  for (const [bytes, message] of refusals) {
    assert.throws(() => setFuses(bytes, RELEASE_FUSES), { message })
  }
})

// A driver that launches a fused bundle waits out Playwright's timeout and learns only that the
// launch failed, so the inspect fuse is read on its own, and the others are set the other way
// round from it to show they are not what is read.
test('a debugger can attach unless the inspect fuse is off, whatever the other fuses say', () => {
  const inspectOff = binary([1, SHIPPED])
  setFuses(inspectOff, { EnableNodeCliInspectArguments: false })
  const allButInspect = binary([1, SHIPPED])
  setFuses(allButInspect, { ...RELEASE_FUSES, EnableNodeCliInspectArguments: true })

  assert.equal(acceptsInspect(binary([1, SHIPPED])), true)
  assert.equal(acceptsInspect(allButInspect), true)
  assert.equal(acceptsInspect(inspectOff), false)
  assert.equal(acceptsInspect(binary([1, SHIPPED], [1, '000011011'])), false)
})

// An installer reads this to refuse a bundle nobody fused, which is what a debug bundle under the
// release's directory name looks like. A universal binary with one slice fused is not fused, and
// neither is a wire that has lost a fuse a release sets.
test('a binary reads as fused only when every wire says what a release sets', () => {
  const fused = binary([1, SHIPPED], [1, SHIPPED])
  setFuses(fused, RELEASE_FUSES)
  assert.equal(hasFuses(fused, RELEASE_FUSES), true)
  assert.equal(hasFuses(binary([1, SHIPPED]), RELEASE_FUSES), false)
  assert.equal(hasFuses(binary([1, '000011011'], [1, SHIPPED]), RELEASE_FUSES), false)
  assert.equal(hasFuses(binary([1, 'r00011011']), RELEASE_FUSES), false)
  // Four fuses long, with the bytes after it reading as the rest of a fused wire.
  const short = Buffer.concat([Buffer.from(SENTINEL), Buffer.from([1, 4]), Buffer.from('000011011', 'latin1')])
  assert.equal(hasFuses(short, RELEASE_FUSES), false)
})

// The wire's layout is Electron's to change, so the fixture above only shows this writes the
// format it was written against. This is the Electron the lockfile installs, which is the one a
// release packages.
test('the Electron this lockfile installs has a wire every release fuse can be set in', (t) => {
  const dist = fileURLToPath(new URL('../node_modules/electron/dist/', import.meta.url))
  const path = process.platform === 'darwin'
    ? `${dist}Electron.app/Contents/Frameworks/Electron Framework.framework/Electron Framework`
    : `${dist}electron`
  if (!existsSync(path)) return t.skip(`no Electron at ${path}: npm ci runs its install script`)
  const bytes = readFileSync(path)
  setFuses(bytes, RELEASE_FUSES)
  // Positions 0, 2 and 3 off, 4 and 5 on; 1 is cookie encryption, which a release leaves alone.
  for (const wire of readFuses(bytes)) assert.equal([0, 2, 3, 4, 5].map((i) => wire[i]).join(''), '00011')
})
