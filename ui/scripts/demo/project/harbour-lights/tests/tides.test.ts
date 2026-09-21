import { test } from 'node:test'
import assert from 'node:assert/strict'
import { heightAt, turningPoints } from '../src/lib/tides'

const WHITBY = [1.94, 0.62, 0.14, 0.11]

test('height stays inside the sum of the amplitudes', () => {
  const bound = WHITBY.reduce((a, b) => a + b, 0)
  for (let hour = 0; hour < 48; hour++) {
    const height = heightAt(WHITBY, hour * 3_600_000)
    assert.ok(Math.abs(height) <= bound + 1e-9, `${height} outside ±${bound}`)
  }
})

test('a flat station never moves', () => {
  assert.equal(heightAt([0, 0, 0, 0], Date.now()), 0)
})

test('turning points come back as clock times', () => {
  const [low, high] = turningPoints(WHITBY, 0, 12)
  assert.match(low, /^\d\d:\d\d$/)
  assert.match(high, /^\d\d:\d\d$/)
})
