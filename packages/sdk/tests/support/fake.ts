import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const FIXTURE = join(here, '../../../test-fixtures/fake-bravebot.mjs')

export interface Fake {
  /** An executable that runs the stand-in in `scenario`. */
  command: string
  env: NodeJS.ProcessEnv
  argv(): string[]
  stdin(): string[]
  dispose(): void
}

/** A stand-in for the `bravebot` command that writes what `scenario` says (see test-fixtures/fake-bravebot.mjs). */
export function fake(scenario: string): Fake {
  const scratch = realpathSync(mkdtempSync(join(tmpdir(), 'sdk-fake-')))
  mkdirSync(scratch, { recursive: true })
  const command = join(scratch, 'bravebot')
  writeFileSync(command, `#!/bin/sh\nexec "${process.execPath}" "${FIXTURE}" "$@"\n`, { mode: 0o755 })
  const argvFile = join(scratch, 'argv.json')
  const stdinFile = join(scratch, 'stdin.txt')
  return {
    command,
    env: { PATH: process.env.PATH, FAKE_SCENARIO: scenario, FAKE_ARGV_FILE: argvFile, FAKE_STDIN_FILE: stdinFile },
    argv: () => JSON.parse(readFileSync(argvFile, 'utf8')) as string[],
    stdin: () => {
      try {
        return readFileSync(stdinFile, 'utf8').split('\n').filter((line) => line !== '')
      } catch {
        return []
      }
    },
    dispose: () => rmSync(scratch, { recursive: true, force: true }),
  }
}
