/**
 * Run a callback the client does not own, whether it returns, throws or returns a promise that
 * rejects. A failure goes to `onFailure` and goes no further, so no callback can stop the client
 * from reading its stream, ending its views or rejecting its requests.
 */
export function isolate(callback: () => unknown, onFailure: (error: unknown) => void): void {
  const failed = (error: unknown): void => {
    try {
      onFailure(error)
    } catch {
      // The report of a failure is not allowed to fail either.
    }
  }
  try {
    const result = callback() as PromiseLike<unknown> | undefined | null
    if (result && typeof result.then === 'function') result.then(undefined, failed)
  } catch (error) {
    failed(error)
  }
}

/** "<what>" or "<what>: <detail>", for reporting a failure whose text may not be readable. */
export function describeThrown(what: string, error: unknown): string {
  const detail = describeFailure(error)
  return detail === null ? what : `${what}: ${detail}`
}

/** The text of a thrown value, or null when it cannot be converted to text. */
export function describeFailure(error: unknown): string | null {
  try {
    return error instanceof Error ? error.message : String(error)
  } catch {
    return null
  }
}
