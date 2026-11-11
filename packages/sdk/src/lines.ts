import { StringDecoder } from 'node:string_decoder'
import { ProtocolError } from './errors.js'
import type { BravebotEvent, Message, RunResult } from './types.js'

/**
 * Splits bytes into lines at `\n` only, keeping a character that arrives in two chunks whole.
 *
 * Nothing but `\n` ends a line, because the strings in a line are escaped and a separator that is
 * allowed inside one (such as U+2028) must not split it.
 */
export class LineSplitter {
  private readonly decoder = new StringDecoder('utf8')
  private rest = ''

  push(chunk: Buffer): string[] {
    this.rest += this.decoder.write(chunk)
    const parts = this.rest.split('\n')
    this.rest = parts.pop() ?? ''
    return parts
  }

  /** The unfinished line left when the stream ended, if there is one. */
  end(): string[] {
    this.rest += this.decoder.end()
    const last = this.rest
    this.rest = ''
    return last === '' ? [] : [last]
  }
}

/**
 * Reads one line of stdout. An empty line is nothing and gives `null`.
 *
 * An object with an `event` field is an event and one without is the result object (CLI-24). The
 * library reads only the envelope: `event`, and `ok` and `status` of the result. Everything else is
 * carried as it was written.
 */
export function parseLine(line: string, number: number): Message | null {
  const text = line.endsWith('\r') ? line.slice(0, -1) : line
  if (text.trim() === '') return null
  let value: unknown
  try {
    value = JSON.parse(text)
  } catch {
    throw new ProtocolError(`line ${number} of the output is not JSON`)
  }
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new ProtocolError(`line ${number} of the output is not an object`)
  }
  const object = value as Record<string, unknown>
  if ('event' in object) {
    if (typeof object.event !== 'string') throw new ProtocolError(`line ${number} of the output has an event that is not a name`)
    return { type: 'event', event: object as unknown as BravebotEvent }
  }
  if (typeof object.ok !== 'boolean' || typeof object.status !== 'number') {
    throw new ProtocolError(`line ${number} of the output is neither an event nor a result`)
  }
  return { type: 'result', result: object as unknown as RunResult }
}
