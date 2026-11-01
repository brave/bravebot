/** An option or argument the library refuses to turn into a command line. Nothing was started. */
export class ArgumentError extends Error {
  override name = 'ArgumentError'
}

/**
 * A line on the bravebot process's stdout that is not one of the objects the event stream writes.
 *
 * The message names the line by its number and never repeats the line.
 */
export class ProtocolError extends Error {
  override name = 'ProtocolError'
}

/** The bravebot process could not be started, was stopped, or ended before it wrote what was asked of it. */
export class ProcessError extends Error {
  override name = 'ProcessError'
  readonly code: number | null
  readonly signal: NodeJS.Signals | null

  constructor(message: string, details: { code?: number | null; signal?: NodeJS.Signals | null; cause?: unknown } = {}) {
    super(message, details.cause === undefined ? undefined : { cause: details.cause })
    this.code = details.code ?? null
    this.signal = details.signal ?? null
  }
}
