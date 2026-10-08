import assert from 'node:assert/strict'
import { PassThrough } from 'node:stream'
import { test } from 'node:test'
import { yesNoAsker } from '../scripts/ask.js'

const tick = (): Promise<void> => new Promise((resolve) => setImmediate(resolve))

test('many questions on one terminal are each answered, with no warning about piled-up listeners', async () => {
  const warnings: Error[] = []
  const note = (warning: Error): number => warnings.push(warning)
  process.on('warning', note)
  const input = new PassThrough()
  const asker = yesNoAsker(input, new PassThrough())
  try {
    for (let at = 0; at < 30; at++) {
      const answer = asker.ask(`question ${at}? `)
      input.write(at % 2 === 0 ? 'y\n' : 'n\n')
      assert.equal(await answer, at % 2 === 0, `question ${at}`)
    }
    await tick()
    assert.deepEqual(warnings.map((warning) => warning.name), [])
  } finally {
    process.off('warning', note)
    asker.close()
  }
})

test('input ending answers the pending question and every later one as no', async () => {
  const input = new PassThrough()
  const asker = yesNoAsker(input, new PassThrough())
  const pending = asker.ask('first? ')
  input.end()
  assert.equal(await pending, false)
  assert.equal(await asker.ask('second? '), false)
  asker.close()
})
