/** What crosses between the window and the main process for a bug report (DIAG-9). */

/** What `bug-report.compose` answers: the text, or `available: false` and none. */
export interface Composed {
  available: boolean
  text: string
  /** The newest diagnostic log's path, to attach separately, or nothing where there is none. */
  log: string | null
}

export type SaveOutcome =
  | { status: 'saved'; where: string }
  | { status: 'cancelled' }
  | { status: 'failed'; message: string }
