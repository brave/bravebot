import { markersOf } from './interjecting'
import type { Live } from './live'
import { RequestFailed } from './rewind'
import { named, sent, type Staged } from './staging'
import * as t from './transcript'

/** The terminal's note for `/manifest` with no task (`manifest_needs_a_task`), and the bridge's refusal. */
export const NEEDS_A_TASK = 'a manifest run needs a task to plan'

/** Why a `/manifest` line with paperclip files on it is put back. */
export const NO_PAPERCLIP = 'A /manifest task cannot carry files chosen with the paperclip. Take them off, or name the files in the task.'

/** Why a `/manifest` line with a pasted picture on it is put back. */
export const NO_PASTED_PICTURE = 'A /manifest task cannot carry a pasted picture. Take it off, or drop the picture onto the task instead.'

/** Why a `/manifest` line with a folded paste or a summoned bot on it is put back. */
export const ONLY_DROPS = 'A /manifest task carries only the files dropped onto it: its plan is fixed before anything is read. Take the rest off and write it in the task instead.'

/** What starting a run needs from the window, passed in so the tests can run it without one. */
export interface PlanRunning {
  call: <T>(method: string, params: Record<string, unknown>) => Promise<T>
  read: (handle: string) => Live | undefined
  update: (handle: string, change: (old: Live) => Live) => void
  /** Put the line back in the composer, if `handle` is the chat on screen. */
  putBack: (handle: string, line: string) => void
  setProblem: (problem: string | null) => void
}

/** Why a typed `/manifest` line cannot go as it stands, or null when it can. */
function refusal(live: Live | undefined, staged: Staged[]): string | null {
  if ((live?.attachments?.length ?? 0) > 0) return NO_PAPERCLIP
  if (staged.some((item) => item.via === 'paste')) return NO_PASTED_PICTURE
  if (staged.some((item) => item.via !== 'drop')) return ONLY_DROPS
  return null
}

/**
 * Start a plan-then-execute run for a typed `/manifest <task>` line (CMD-1, MANIFEST-9), with what
 * was dropped on it (DROP-4).
 *
 * The task goes to `manifest.run` and never as a prompt. Its row is drawn before the call, because
 * the run's first events can arrive before the call returns, and it is taken off again if the bridge
 * refuses. It is a `plan-task` row, not a `user` one, so it is no turn of the conversation
 * (MANIFEST-11). A run refused while a turn runs (`turn_in_flight`, the bridge's decision) waits in
 * the queue with what it named, and is asked again when the turn ends.
 *
 * Pictures and PDFs dropped on the line go as the grant ids the main process resolves into the
 * run's `attachments`, and a dropped text file as its marker and name, which the bridge puts back
 * into the task. A file chosen with the paperclip is a project file the plan cannot read before it
 * is fixed, and a pasted picture, folded paste or summoned bot is not taken on a run from the
 * window, so a line with any of them goes back in the composer and nothing is sent.
 */
export async function planRun(on: PlanRunning, handle: string, task: string, line: string, queuedAlready = false, queuedStaged: Staged[] = []): Promise<void> {
  if (task === '') {
    on.update(handle, (old) => ({ ...old, entries: [...old.entries, t.narrated(NEEDS_A_TASK)] }))
    return
  }
  const asking = on.read(handle)
  // A line from the queue carries what it named; what the box holds now is the next draft's.
  const staged = queuedAlready ? queuedStaged : named(asking?.staging, line)
  const refused = queuedAlready ? null : refusal(asking, staged)
  if (refused) {
    on.putBack(handle, line)
    on.setProblem(refused)
    return
  }
  // Each goes with its marker, which the bridge titles the run by: the plan card and the record
  // name the file where that marker stands.
  const pictures = staged
    .flatMap((item) => item.via === 'drop' && item.file.kind !== 'text' ? [{ marker: item.marker, file: item.file }] : [])
  const drops = pictures.map((item) => item.file.id)
  const markers = pictures.map((item) => item.marker)
  const files = markersOf(staged.filter((item) => item.via === 'drop' && item.file.kind === 'text')).files
  const row = t.planAsked(task)
  const without = (old: Live) => old.entries.filter((entry) => entry.id !== row.id)
  // Set before the call, since the plan card can arrive before the call's answer does.
  on.update(handle, (old) => ({ ...old, planDropped: pictures, entries: [...old.entries, row] }))
  try {
    await on.call('manifest.run', { session: handle, task, model: asking?.model ?? null, drops, markers, files })
    // What was sent leaves the composer.
    if (!queuedAlready) on.update(handle, (old) => ({ ...old, staging: sent(old.staging) }))
  } catch (error) {
    if (error instanceof RequestFailed && error.code === 'turn_in_flight' && !queuedAlready) {
      on.update(handle, (old) => ({ ...old, planDropped: undefined, entries: without(old), queued: [...(old.queued ?? []), { prompt: line, attachments: [], staged, manifest: task }], staging: sent(old.staging) }))
      return
    }
    // A refused line with drops on it goes back in the composer, where its markers still resolve.
    if (!queuedAlready && staged.length > 0) on.putBack(handle, line)
    const reason = error instanceof RequestFailed ? error.reason : String(error)
    on.update(handle, (old) => ({ ...old, planDropped: undefined, entries: [...without(old), t.errored(reason)] }))
  }
}
