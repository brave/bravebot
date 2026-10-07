import assert from 'node:assert/strict'
import { execFile } from 'node:child_process'
import { dirname, join } from 'node:path'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'
import { promisify } from 'node:util'

const root = join(dirname(fileURLToPath(import.meta.url)), '../..')

test('a separate strict TypeScript project can import both entry points with their types', async () => {
  const tsc = join(root, 'node_modules/typescript/bin/tsc')
  const result = await promisify(execFile)(process.execPath, [tsc, '-p', join(root, 'test-fixtures/consumer/tsconfig.json')]).then(
    () => 'ok',
    (failed: { stdout: string }) => failed.stdout,
  )
  assert.equal(result, 'ok')
})
