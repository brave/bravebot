import { withDrops } from './drops'
import { attachmentPaths } from './files'

/**
 * The params a method is allowed to have arrived with.
 *
 * `turn.send` takes three lists of file paths, and all of them are admitted to the planner as
 * *trusted* input: `files`, read inside the workspace; `dropped`, read as text anywhere on the
 * disk; and `attachments`, pictures and PDFs read as bytes anywhere on the disk. It also takes
 * `images`, pasted pictures sent as bytes on the footing of the prompt. Nothing else the renderer
 * can say has that reach: the file tree is confined to roots this process learnt from the agent,
 * the folder picker is native, and the preload has never carried a file's contents in either
 * direction. A window that could name any of these would be a window that could read any file on
 * the machine, or put any bytes in front of the planner as the person's own, which is a larger
 * change than any feature is worth.
 *
 * So they are removed here rather than trusted here, and `files` is composed again from the
 * tokens the native picker left. The bridge adds the names the prompt gives with `@`, read out of
 * the prompt and confined to the session's workspace by the read the turn makes (NAME-9). A window
 * can name a project file by writing it into the prompt, as a person typing `@` in the terminal
 * does, and it cannot name anything outside the workspace or put a path into any list directly.
 * A bot's turn needs `files` and `dropped`, and gets them from `bravebot:bots:send`, which
 * composes the paths itself from a definition this process holds and never from anything that
 * crossed the bridge from a window.
 *
 * The window's own `attachments` is a different list from the agent's: ids of project files the
 * person chose in a native picker, which this process resolves to `files`. It is never forwarded
 * under its own name, so an id list cannot become a list of paths the agent reads as bytes.
 *
 * `drops` is the window's list of files a person dropped, again as ids: grants `drops.ts` minted
 * from paths the preload took off a trusted drop event, which the page never saw. This process
 * turns them into the agent's `dropped` and `attachments` after everything the window said under
 * those names is gone, so the only paths in either are ones a drop put there.
 *
 * Stripped silently. There is no legitimate caller to warn, and a message saying which key was
 * removed would be a message telling a compromised renderer what to try next.
 */
export function sanitised(method: string, params: unknown): Record<string, unknown> {
  const held = (params ?? {}) as Record<string, unknown>
  // A manifest run takes a task and no files. Only the three fields it reads are forwarded, so
  // a window cannot name a file to it under any key. A name written with `@` stays a word of the
  // task, as it does for `/manifest` in the terminal: a plan is fixed before anything is read.
  if (method === 'manifest.run') {
    return { session: held.session, task: held.task, model: held.model }
  }
  if (method !== 'turn.send') return held
  // `recall` and `definition` join the lists for a smaller reason than theirs. `recall` decides
  // whether a prompt is one a person can find again, and `definition` decides which definition a
  // turn is addressed to (MEMORY-10). Both are claims about who asked and what the turn is for,
  // which this process makes from a bot's row and a window does not get to. A window that could
  // name a definition could address a turn to any definition on the machine.
  const {
    files: _files,
    dropped: _dropped,
    images: _images,
    recall: _recall,
    definition: _definition,
    attachments,
    drops,
    ...rest
  } = held
  const session = typeof rest.session === 'string' ? rest.session : ''
  return withDrops(session, drops, { ...rest, files: attachmentPaths(session, attachments) })
}
