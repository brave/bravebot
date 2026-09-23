import { mkdirSync, lstatSync } from 'node:fs'
import { replaceAgentHooks } from './project-files'

function realDirectory(home: string): void {
  const stat = lstatSync(home)
  if (!stat.isDirectory() || stat.isSymbolicLink()) throw new Error('The agent state directory must be a real directory.')
}
/**
 * Write the file and say nothing about what is in it: the caller asks the agent that, so this
 * process holds no opinion about what a hook is. `expected` is the text the agent last reported,
 * and the write refuses when the file no longer holds it.
 */
export function saveHooks(home: string, text: string, expected: string | null): void {
  mkdirSync(home, { recursive: true, mode: 0o700 })
  realDirectory(home)
  // Pins each directory and atomically replaces the fixed leaf, refusing symlinks and stale text.
  replaceAgentHooks(home, text, expected)
}
