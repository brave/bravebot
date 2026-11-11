import { ArgumentError } from './errors.js'
import type { CommonOptions, SessionOptions } from './types.js'

/**
 * Every option the library accepts, by the flag it becomes (or, for those that are not flags,
 * the way the process is started). Anything else is refused, so a caller cannot reach a flag that
 * lowers a permission, vouches for a path or writes the trust map by passing it under another name.
 */
export const FLAG_OPTIONS = {
  model: '--model',
  effort: '--effort',
  outputSchema: '--output-schema',
  maxRounds: '--max-rounds',
  maxTokens: '--max-tokens',
  tools: '--tools',
  noShell: '--no-shell',
  systemPrompt: '--system-prompt',
  incognito: '--incognito',
  resume: '--resume',
} as const

/** Options that start or watch the process and become no flag. */
export const PROCESS_OPTIONS = ['command', 'cwd', 'env', 'signal', 'onStderr', 'onEvent'] as const

/** Options that only a session takes. `trustInputLines` is a flag, and the other two are callbacks. */
export const SESSION_ONLY_OPTIONS = ['trustInputLines', 'onQuestion', 'onError'] as const

export const RUN_OPTION_NAMES: readonly string[] = [...Object.keys(FLAG_OPTIONS), ...PROCESS_OPTIONS].sort()
export const SESSION_OPTION_NAMES: readonly string[] = [...RUN_OPTION_NAMES, ...SESSION_ONLY_OPTIONS].sort()

function text(name: string, value: unknown): string {
  if (typeof value !== 'string' || value.trim() === '') throw new ArgumentError(`${name} must be a non-empty string`)
  // The command line reads a word that starts with `-` as a flag, so it is refused here, before
  // any process is started, rather than being turned into a different command.
  if (value.startsWith('-')) throw new ArgumentError(`${name} must not start with "-"`)
  return value
}

function count(name: string, value: unknown): string {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 1) {
    throw new ArgumentError(`${name} must be a positive integer`)
  }
  return String(value)
}

function refuseUnknown(options: object, allowed: readonly string[]): void {
  for (const name of Object.keys(options)) {
    if (!allowed.includes(name)) throw new ArgumentError(`${name} is not an option of this library`)
  }
}

function flagsOf(options: CommonOptions): string[] {
  const flags: string[] = []
  if (options.model !== undefined) flags.push('--model', text('model', options.model))
  if (options.effort !== undefined) flags.push('--effort', text('effort', options.effort))
  if (options.outputSchema !== undefined) flags.push('--output-schema', text('outputSchema', options.outputSchema))
  if (options.maxRounds !== undefined) flags.push('--max-rounds', count('maxRounds', options.maxRounds))
  if (options.maxTokens !== undefined) flags.push('--max-tokens', count('maxTokens', options.maxTokens))
  if (options.tools !== undefined) {
    if (!Array.isArray(options.tools) || options.tools.length === 0) throw new ArgumentError('tools must list at least one name')
    const names = options.tools.map((name) => {
      const word = text('a tool name', name)
      if (word.includes(',')) throw new ArgumentError('a tool name must not hold a comma')
      return word
    })
    flags.push('--tools', names.join(','))
  }
  if (options.noShell === true) flags.push('--no-shell')
  if (options.systemPrompt !== undefined) flags.push('--system-prompt', text('systemPrompt', options.systemPrompt))
  if (options.incognito === true) flags.push('--incognito')
  if (options.resume !== undefined) flags.push('--resume', text('resume', options.resume))
  return flags
}

/** The command line of a one-shot run: one `--json-stream` run of one prompt (CLI-24). */
export function runArguments(prompt: string, options: CommonOptions): string[] {
  refuseUnknown(options, RUN_OPTION_NAMES)
  return ['--json-stream', ...flagsOf(options), '-p', text('the prompt', prompt)]
}

/** The command line of a session: one process reading turns from stdin (CLI-36). */
export function sessionArguments(options: SessionOptions): string[] {
  refuseUnknown(options, SESSION_OPTION_NAMES)
  const flags = ['--json-stream', '--input-format', 'lines']
  if (options.trustInputLines === true) flags.push('--trust-input-lines')
  return [...flags, ...flagsOf(options)]
}
