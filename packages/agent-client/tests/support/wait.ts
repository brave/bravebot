/** A promise that rejects after `ms`, so a test that never reaches its state fails instead of hanging. */
export function within<T>(promise: Promise<T>, what: string, ms = 10_000): Promise<T> {
  let timer: NodeJS.Timeout | undefined
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(`timed out after ${ms}ms waiting for ${what}`)), ms)
  })
  return Promise.race([promise, timeout]).finally(() => clearTimeout(timer))
}

/** Matches `expected` as a subset of `actual`: objects by their listed keys, arrays by length and element. */
export function subsetMismatch(actual: unknown, expected: unknown, path = '$'): string | null {
  if (Array.isArray(expected)) {
    if (!Array.isArray(actual) || actual.length !== expected.length) {
      return `${path}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`
    }
    for (let at = 0; at < expected.length; at++) {
      const mismatch = subsetMismatch(actual[at], expected[at], `${path}[${at}]`)
      if (mismatch) return mismatch
    }
    return null
  }
  if (typeof expected === 'object' && expected !== null) {
    if (typeof actual !== 'object' || actual === null || Array.isArray(actual)) {
      return `${path}: expected an object, got ${JSON.stringify(actual)}`
    }
    for (const [key, value] of Object.entries(expected)) {
      const mismatch = subsetMismatch((actual as Record<string, unknown>)[key], value, `${path}.${key}`)
      if (mismatch) return mismatch
    }
    return null
  }
  return actual === expected ? null : `${path}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`
}
