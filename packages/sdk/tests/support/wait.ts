/** A promise that rejects after `ms`, so a test that never reaches its state fails instead of hanging. */
export function within<T>(promise: Promise<T>, what: string, ms = 30_000): Promise<T> {
  let timer: NodeJS.Timeout | undefined
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(`timed out after ${ms}ms waiting for ${what}`)), ms)
  })
  return Promise.race([promise, timeout]).finally(() => clearTimeout(timer))
}
