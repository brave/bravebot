// The demo's recording helpers. `createRecorder` needs macOS's `screencapture` and a window, so
// only its no-window path runs here; `writeSubtitles` is pure apart from the file it writes.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { createRecorder, writeSubtitles } from './demo/recording.mjs'

test('writeSubtitles writes one cue per caption, ending where the next begins', () => {
  const dir = mkdtempSync(join(tmpdir(), 'demo-recording-'))
  try {
    writeSubtitles(['00:03  Open: x', '00:09  Next'], join(dir, 'take.mp4'))
    assert.equal(
      readFileSync(join(dir, 'take.vtt'), 'utf8'),
      [
        'WEBVTT',
        '',
        '00:00:03.000 --> 00:00:09.000',
        'Open: x',
        '',
        // The last cue has no successor and lasts six seconds.
        '00:00:09.000 --> 00:00:15.000',
        'Next',
        '',
      ].join('\n'),
    )
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test('writeSubtitles gives a cue that shares its second with the next one a second of its own', () => {
  const dir = mkdtempSync(join(tmpdir(), 'demo-recording-'))
  try {
    writeSubtitles(['01:01  First', '01:01  Second', '01:05  Third', '70:00  Past an hour'], join(dir, 'a.b.mov'))
    const lines = readFileSync(join(dir, 'a.b.vtt'), 'utf8').split('\n')
    assert.equal(lines[2], '00:01:01.000 --> 00:01:02.000')
    assert.equal(lines[3], 'First')
    assert.equal(lines[5], '00:01:01.000 --> 00:01:05.000')
    assert.equal(lines[8], '00:01:05.000 --> 01:10:00.000')
    assert.equal(lines[11], '01:10:00.000 --> 01:10:06.000')
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test('a recorder with no window returns no start time, and stop has nothing to return', async () => {
  const app = { evaluate: async () => null }
  const page = { waitForTimeout: async () => assert.fail('waited for a recorder that never started') }
  const log = console.log
  const said = []
  console.log = (line) => {
    said.push(line)
  }
  try {
    const recorder = createRecorder({ app, page, opts: {} })
    assert.equal(await recorder.record('/nowhere/take.mov'), undefined)
    assert.equal(await recorder.stop(), null)
  } finally {
    console.log = log
  }
  assert.deepEqual(said, ['  not recording: there is no window to record'])
})
