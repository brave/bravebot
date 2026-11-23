import type { Live } from './live'
import { RequestFailed } from './rewind'
import { named } from './staging'
import * as t from './transcript'

/** The terminal's note for `/manifest` with no task (`manifest_needs_a_task`), and the bridge's refusal. */
export const NEEDS_A_TASK = 'a manifest run needs a task to plan'

/** Why a `/manifest` line with files on it is put back, in the words of the bridge's refusal. */
export const TAKES_NO_FILES = 'A plan run takes no files: its plan is fixed before anything is read. Take them off and name the file in the task instead.'

/** What starting a run needs from the window, passed in so the tests can run it without one. */
export interface PlanRunning {
  call: <T>(method: string, params: Record<string, unknown>) => Promise<T>
  read: (handle: string) => Live | undefined
  update: (handle: string, change: (old: Live) => Live) => void
  /** Put the line back in the composer, if `handle` is the chat on screen. */
  putBack: (handle: string, line: string) => void
  setProblem: (problem: string | null) => void
}

/**
 * Start a plan-then-execute run for a typed `/manifest <task>` line (CMD-1, MANIFEST-9).
 *
 * The task goes to `manifest.run` and never as a prompt. Its row is drawn before the call, because
 * the run's first events can arrive before the call returns, and it is taken off again if the bridge
 * refuses. It is a `plan-task` row, not a `user` one, so it is no turn of the conversation
 * (MANIFEST-11). A run refused while a turn runs (`turn_in_flight`, the bridge's decision) waits in
 * the queue and is asked again when the turn ends.
 *
 * A run fixes its plan before anything is read, so the bridge refuses files and pictures. A line with
 * paperclip files or staged items goes back in the composer with them, and nothing is sent.
 */
export async function planRun(on: PlanRunning, handle: string, task: string, line: string, queuedAlready = false): Promise<void> {
  if (task === '') {
    on.update(handle, (old) => ({ ...old, entries: [...old.entries, t.narrated(NEEDS_A_TASK)] }))
    return
  }
  const asking = on.read(handle)
  if (!queuedAlready && ((asking?.attachments?.length ?? 0) > 0 || named(asking?.staging, line).length > 0)) {
    on.putBack(handle, line)
    on.setProblem(TAKES_NO_FILES)
    return
  }
  const row = t.planAsked(task)
  const without = (old: Live) => old.entries.filter((entry) => entry.id !== row.id)
  on.update(handle, (old) => ({ ...old, entries: [...old.entries, row] }))
  try {
    await on.call('manifest.run', { session: handle, task, model: asking?.model ?? null })
  } catch (error) {
    if (error instanceof RequestFailed && error.code === 'turn_in_flight' && !queuedAlready) {
      on.update(handle, (old) => ({ ...old, entries: without(old), queued: [...(old.queued ?? []), { prompt: line, attachments: [], staged: [], manifest: task }] }))
      return
    }
    const reason = error instanceof RequestFailed ? error.reason : String(error)
    on.update(handle, (old) => ({ ...old, entries: [...without(old), t.errored(reason)] }))
  }
}
