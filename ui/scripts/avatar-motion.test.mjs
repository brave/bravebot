import test from 'node:test'
import assert from 'node:assert/strict'
import { blink, glance, completionNod } from '../src/renderer/avatar/motion.ts'

test('idle glances leave most of the cycle completely still and never exceed the turn limit', () => {
  for (const seed of ['blue', 'yellow', 'quiet']) {
    let still = 0
    for (let tick = 0; tick < 2400; tick++) {
      const value = glance(tick / 100, seed)
      assert.ok(Math.abs(value) <= 1)
      if (value === 0) still++
    }
    assert.ok(still / 2400 > 0.75)
    for (let boundary = 12; boundary < 120; boundary += 12) {
      assert.ok(glance(boundary - 0.001, seed) === 0)
      assert.ok(glance(boundary, seed) === 0)
    }
  }
})

test('blinks vary their intervals, stay bounded, and are reproducible across views', () => {
  const starts = []
  let wasOpen = true
  for (let tick = 0; tick < 14000; tick++) {
    const seconds = tick / 100
    const value = blink(seconds, 'same-bot')
    assert.ok(value >= 0.08 && value <= 1)
    assert.equal(value, blink(seconds, 'same-bot'))
    if (value < 1 && wasOpen) starts.push(seconds)
    wasOpen = value === 1
  }
  const intervals = starts.slice(1).map((time, i) => time - starts[i])
  assert.ok(starts.length >= 20)
  assert.ok(intervals.some(interval => interval < 0.5), 'occasional double blink')
  assert.ok(new Set(intervals.filter(interval => interval > 1).map(Math.round)).size > 3)
})

test('completion pauses for eye contact before one smooth nod', () => {
  for (const time of [0, 0.2, 0.35, 0.5, 0.55, 1.05, 2, 20]) assert.equal(completionNod(time), 0)
  assert.ok(completionNod(0.8) > 0.2)
  assert.ok(completionNod(0.56) < 0.01)
  assert.ok(completionNod(1.04) < 0.01)
})
