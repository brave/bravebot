import type { LanguageReport } from '../../shared/protocol'

/** How long the first paint waits on the bridge to say which catalog to read. */
export const LANGUAGE_WAIT_MS = 3000

/**
 * The tag the bridge says the window is read in. A bridge that fails, answers something else, or
 * does not answer within `wait` leaves the reference rather than a blank window.
 */
export async function askLocale(
  request: () => Promise<{ ok?: LanguageReport | null } | null | undefined>,
  wait = LANGUAGE_WAIT_MS,
): Promise<string> {
  let timer: ReturnType<typeof setTimeout> | undefined
  const late = new Promise<null>((resolve) => { timer = setTimeout(() => resolve(null), wait) })
  try {
    const answer = await Promise.race([request(), late])
    return typeof answer?.ok?.inUse === 'string' ? answer.ok.inUse : 'en-US'
  } catch {
    return 'en-US'
  } finally {
    clearTimeout(timer)
  }
}
