/**
 * The questions a chat asks once its trust question is answered, about what the checkout's
 * settings files ask for (PERM-15, TRUST-9, §9.1 of the protocol): one about the `allow` rules
 * they propose, then one about each directory they name.
 *
 * Nothing here decides anything. The bridge puts each question, says which comes next in the
 * answer to the last, and grants or opens only what a yes to the question it put covers. The
 * window answers with the request id and a yes or a no, never with a rule or a path of its own,
 * and draws whichever question the bridge last said is out.
 */
import type {
  FolderQuestion,
  FolderReplied,
  ProjectQuestionsReport,
  RefusedFolder,
  RulesQuestion,
  RulesReplied,
  SettingsRules,
} from '../shared/protocol'
import { t } from './i18n'

/** What a session holds of the questions, in the names `Live` keeps them under. */
export interface Asking {
  rulesQuestion: RulesQuestion | null
  folderQuestion: FolderQuestion | null
  foldersRefused: RefusedFolder[]
}

/** What a report changes about a session: the questions always, the folders and rules where it carries them. */
export type Reported = Asking & { folders?: string[]; rules?: SettingsRules | null }

/**
 * What a response that may make a question due says (session.new, .open, .fork, trust.reply, and
 * permissions.list). A bridge that puts none of these questions carries none of the fields, and
 * then none is out.
 */
export function reported(answer: ProjectQuestionsReport | null | undefined): Reported {
  const out: Reported = {
    rulesQuestion: answer?.rulesQuestion ?? null,
    folderQuestion: answer?.folderQuestion ?? null,
    foldersRefused: Array.isArray(answer?.foldersRefused) ? answer.foldersRefused : [],
  }
  if (Array.isArray(answer?.folders)) out.folders = answer.folders.filter((each): each is string => typeof each === 'string')
  if (answer?.settingsRules !== undefined) out.rules = answer.settingsRules
  return out
}

/** Which question stops the composer, in the order they are asked: trust, then rules, then folders. */
export type Waiting = 'trust' | 'rules' | 'folder'

export function waitingOn(live: { askingTrust: string | null } & Partial<Asking>): Waiting | null {
  if (live.askingTrust) return 'trust'
  if (live.rulesQuestion) return 'rules'
  if (live.folderQuestion) return 'folder'
  return null
}

/** What the composer says while a question stops it from sending. */
export function waitingWords(waiting: Waiting | null): string | null {
  if (waiting === 'trust') return t.uiProjectQuestionsWaitingTrust()
  if (waiting === 'rules') return t.uiProjectQuestionsWaitingRules()
  if (waiting === 'folder') return t.uiProjectQuestionsWaitingFolder()
  return null
}

/** What a `rules.reply` answer changes: the rules question is gone, and the first folder question may be out. */
export function afterRules(replied: RulesReplied): Partial<Reported> {
  return { rulesQuestion: null, folderQuestion: replied.folderQuestion ?? null, rules: replied.settingsRules ?? null }
}

/**
 * What a `folder.reply` answer changes: the next folder question, the folders open now, and a
 * yes that could not open the folder joining the ones not opened, as the bridge adds it there.
 */
export function afterFolder(replied: FolderReplied, refused: RefusedFolder[]): Partial<Reported> {
  const change: Partial<Reported> = { folderQuestion: replied.folderQuestion ?? null }
  if (Array.isArray(replied.folders)) change.folders = replied.folders
  if (replied.problem) {
    change.foldersRefused = [...refused, {
      directory: replied.folder,
      shownDirectory: replied.shownFolder,
      problem: replied.problem,
      shownProblem: replied.problem,
    }]
  }
  return change
}

/** Whether a reply was refused because the question it answered is no longer out. */
export function isStale(error: unknown): boolean {
  return typeof error === 'object' && error !== null && (error as { code?: unknown }).code === 'no_such_request'
}

/** What answering needs from the window: the bridge, the session as last drawn, and where to say a failure. */
export interface Answering {
  call: <T>(method: string, params: Record<string, unknown>) => Promise<T>
  /** The session's questions and refusals as the window holds them now. */
  read: (handle: string) => (Partial<Asking> & { askingTrust?: string | null }) | undefined
  update: (handle: string, change: Partial<Reported>) => void
  setProblem: (problem: string) => void
}

/**
 * Read which question is out after a reply was refused as stale, from the permissions list, which
 * carries the same report. Asked once per refusal and never retried: where it fails, the refused
 * question is put away, so a person is not left pressing an answer the bridge will not take, and
 * a turn sent meanwhile is refused by the bridge naming the method that answers what is out.
 */
async function reread(answering: Answering, handle: string, stale: { rules?: number; folder?: number }): Promise<void> {
  answering.setProblem(t.uiProjectQuestionsStale())
  try {
    const listed = await answering.call<ProjectQuestionsReport>('permissions.list', { session: handle })
    answering.update(handle, reported(listed))
  } catch {
    const now = answering.read(handle)
    const change: Partial<Reported> = {}
    if (stale.rules !== undefined && now?.rulesQuestion?.request === stale.rules) change.rulesQuestion = null
    if (stale.folder !== undefined && now?.folderQuestion?.request === stale.folder) change.folderQuestion = null
    answering.update(handle, change)
  }
}

/**
 * Answer the rules question out in `handle`: a yes grants every rule it listed, a no none. Returns
 * once the window holds what the answer led to, which is the first folder question where there is one.
 */
export async function replyRules(answering: Answering, handle: string, granted: boolean): Promise<void> {
  const asked = answering.read(handle)?.rulesQuestion
  if (!asked) return
  try {
    const replied = await answering.call<RulesReplied>('rules.reply', { session: handle, request: asked.request, granted })
    answering.update(handle, afterRules(replied))
    // Still a yes, for this chat. Said, because the next chat here will ask after all.
    if (granted && replied.kept === false && asked.shownKeeping) {
      answering.setProblem(t.uiProjectRulesNotKept({ path: asked.shownKeeping }))
    }
  } catch (error) {
    if (isStale(error)) await reread(answering, handle, { rules: asked.request })
    else answering.setProblem(String(error instanceof Error ? error.message : error))
  }
}

/**
 * Answer the folder question out in `handle`: a yes opens the directory it named, a no leaves it
 * closed. Returns once the window holds the next folder question, if any, and the folders open now.
 * Once the last is answered the permissions list is read again, since the directories still not
 * open are said in the rules the bridge holds and a folder reply does not carry them.
 */
export async function replyFolder(answering: Answering, handle: string, open: boolean): Promise<void> {
  const held = answering.read(handle)
  const asked = held?.folderQuestion
  if (!asked) return
  let replied: FolderReplied
  try {
    replied = await answering.call<FolderReplied>('folder.reply', { session: handle, request: asked.request, open })
  } catch (error) {
    if (isStale(error)) await reread(answering, handle, { folder: asked.request })
    else answering.setProblem(String(error instanceof Error ? error.message : error))
    return
  }
  answering.update(handle, afterFolder(replied, held.foldersRefused ?? []))
  if (open && replied.problem) {
    answering.setProblem(t.uiProjectFolderNotOpened({ path: replied.shownFolder || asked.shownDirectory, problem: replied.problem }))
  }
  if (!replied.folderQuestion) {
    try {
      const listed = await answering.call<ProjectQuestionsReport>('permissions.list', { session: handle })
      answering.update(handle, reported(listed))
    } catch {
      // The folders are current from the reply. Only the folded note's list of directories not
      // open lags, until the rules are next read.
    }
  }
}

/**
 * The directories a settings file named that are not open, each with the agent's sentence where it
 * could not be opened at all, and the refusals the rules do not list by that name (a yes that came
 * too late to open its directory is refused under the directory it resolved to), so none goes unsaid.
 */
export function unopened(rules: SettingsRules | null | undefined, refused: RefusedFolder[] = []): {
  closed: { name: string; shown: string; problem: string | null }[]
  refused: RefusedFolder[]
} {
  const names = rules?.directories ?? []
  const closed = names.map((name, index) => {
    const why = refused.find((each) => each.directory === name)
    return { name, shown: rules?.shownDirectories?.[index] ?? name, problem: why?.shownProblem ?? null }
  })
  return { closed, refused: refused.filter((each) => !names.includes(each.directory)) }
}

/** How many entries of the note say something is not in force. */
export function notInForceCount(rules: SettingsRules, refused: RefusedFolder[] = []): number {
  const { closed, refused: unlisted } = unopened(rules, refused)
  const paths = (rules.filesystem ?? []).filter((entry) => entry.refused !== null).length
  return rules.unreadable.length + rules.proposed.length + closed.length + unlisted.length + paths + (rules.filesystemIgnored?.length ?? 0)
}
