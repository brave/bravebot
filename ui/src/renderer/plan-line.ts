import type { FileAttachment } from '../shared/files'
import type { PermissionMode } from '../shared/protocol'
import type { Live } from './live'
import { RequestFailed } from './rewind'
import type { Staged } from './staging'
import * as t from './transcript'

/** What carrying out a `/plan` line needs from the hook that owns the sessions. */
export interface Planning {
  call: <T>(method: string, params: Record<string, unknown>) => Promise<T>
  read: (handle: string) => Live | undefined
  update: (handle: string, change: (old: Live) => Live) => void
  send: (prompt: string, target?: string, selectedFiles?: FileAttachment[], queuedStaged?: Staged[]) => Promise<void>
}

/**
 * Carry out a typed `/plan [task]` line (MODE-12).
 *
 * While a turn runs, or while another plan line waits, the line waits in the queue and is carried
 * out when it reaches the front. Otherwise the session is set to plan mode through `session.mode`,
 * and the mode the bridge answers is what is stored. Only then, and only with a task, is a turn
 * started, and it is sent the task alone, never the typed line. A mode the bridge did not set
 * starts no turn, so no turn is sent outside plan mode. A line that reached the front of the queue
 * carries no file chosen with the paperclip, as it was queued without one.
 */
export async function carryOutPlan(p: Planning, handle: string, task: string, line: string, staged: Staged[], queuedAlready = false): Promise<void> {
  const session = p.read(handle)
  if (!queuedAlready && (session?.running || session?.queued?.some((item) => item.plan !== undefined))) {
    p.update(handle, (old) => ({ ...old, queued: [...(old.queued ?? []), { prompt: line, attachments: [], staged, plan: task }] }))
    return
  }
  try {
    const { permissionMode } = await p.call<{ permissionMode: PermissionMode }>('session.mode', { session: handle, mode: 'plan' })
    p.update(handle, (old) => ({ ...old, permissionMode }))
  } catch (error) {
    // The card's title and sentence come from the 'plan-mode' row in failure.ts, and the bridge's
    // reason is its technical detail.
    p.update(handle, (old) => ({ ...old, entries: [...old.entries, t.errored(error instanceof RequestFailed ? error.reason : String(error), 'plan-mode')] }))
    return
  }
  if (task !== '') await p.send(task, handle, queuedAlready ? [] : undefined, staged)
}
