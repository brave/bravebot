import type { PageFault, PageFaultKind } from '../shared/window-fault'

/** Longest error name sent. The agent recognises none longer. */
const NAME_MAX = 64

/** Longest text shown as technical details, so one huge message cannot fill the window. */
const DETAILS_MAX = 8000

const fieldsOf = (error: unknown): { name?: unknown; message?: unknown; stack?: unknown } =>
  typeof error === 'object' && error !== null ? error : {}

/** The name an error carries, or `Error` where what was thrown is not one. */
export function nameOf(error: unknown): string {
  const { name } = fieldsOf(error)
  return (typeof name === 'string' && name ? name : 'Error').slice(0, NAME_MAX)
}

/**
 * Tell the main process the page failed, so the agent writes it in the diagnostic log (DIAG-10).
 *
 * The name of the error goes and nothing else. The message and the stack stay in this window,
 * where `detailsOf` shows them to the person.
 */
export function reportFault(kind: PageFaultKind, error: unknown): void {
  const fault: PageFault = { kind, name: nameOf(error) }
  try {
    window.bravebot?.reportFault(fault)
  } catch {
    // A window that cannot say it failed has nothing more to do about it.
  }
}

const textOf = (value: unknown): string => {
  try {
    return String(value)
  } catch {
    return 'unreadable'
  }
}

/**
 * What the person is shown to copy into a bug report: the error, where in the page it was drawing,
 * and the stack. It is drawn as text and never sent anywhere.
 */
export function detailsOf(error: unknown, componentStack?: string | null): string {
  const { message, stack } = fieldsOf(error)
  const head = `${nameOf(error)}: ${typeof message === 'string' ? message : textOf(error)}`
  const trace = typeof stack === 'string' && stack ? (stack.includes(head) ? stack : `${head}\n${stack}`) : head
  const frames = (componentStack ?? '').split('\n').map((line) => line.trim()).filter(Boolean)
  const where = frames.length ? `\n\nComponent stack:\n${frames.map((line) => `    ${line}`).join('\n')}` : ''
  return `${trace}${where}`.slice(0, DETAILS_MAX)
}

/**
 * Report an error nothing caught and a promise nothing handled. The page carries on: only an
 * error React meets while it draws takes the window down, and that is the boundary's to report.
 */
export function watchForFaults(): void {
  // An event with no error is the browser's own notice, such as a resize observer that could not
  // deliver every change, and says nothing about the page.
  window.addEventListener('error', (event) => {
    if (event.error) reportFault('uncaught', event.error)
  })
  window.addEventListener('unhandledrejection', (event) => reportFault('rejection', event.reason))
}
