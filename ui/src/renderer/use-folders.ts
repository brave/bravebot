import { useCallback, useEffect } from 'react'
import { call } from './rpc'
import { RequestFailed } from './rewind'
import { showToast } from './toasts'
import { folderAdded, foldersAnswer, type FoldersAnswer } from './folders'
import { t } from './i18n'
import type { Live } from './live'

/** Where an add was asked for, which is where a refusal of it is said. */
export type AskedFrom = 'panel' | 'menu'

/**
 * Adding, closing and listing the folders a chat has open beside its project (TRUST-9).
 *
 * Every answer the agent gives replaces the chat's list, and nothing else does: the window never
 * adds a folder to the list itself, and never names one for the agent to open. An add goes through
 * the main process's own picker, and a cancelled picker does nothing. A refusal is said where the
 * add or close was asked for: in the Folders section, or in the chat's error toast for the More
 * menu, which closes as it is chosen.
 */
export function useFolders({ handle, handleRef, updateSession, setProblem }: {
  /** The chat on screen. The list is asked for each time this changes. */
  handle: string | undefined
  handleRef: React.RefObject<string | null>
  updateSession: (handle: string, action: React.SetStateAction<Live | null>) => void
  setProblem: (problem: string | null) => void
}): { addFolder: (from: AskedFrom) => Promise<void>; closeFolder: (path: string) => Promise<void> } {
  const take = useCallback((session: string, answer: FoldersAnswer) => {
    updateSession(session, (old) => (old ? {
      ...old,
      folders: answer.folders,
      ...(answer.rewind ? { rewind: answer.rewind } : {}),
      folderProblem: null,
    } : old))
  }, [updateSession])
  const refuse = useCallback((session: string, from: AskedFrom, message: string) => {
    if (from === 'menu') setProblem(message)
    else updateSession(session, (old) => (old ? { ...old, folderProblem: message } : old))
  }, [setProblem, updateSession])

  // Asked whenever a chat comes on screen, whether it was opened, restored or switched to, which
  // is also how the main process learns a restored chat's folders before the tree reads under one.
  useEffect(() => {
    if (!handle) return
    let gone = false
    void call('session.folders.list', { session: handle }).then((value) => {
      const answer = foldersAnswer(value)
      // The problem said about the last add or close is kept: it is still what happened.
      if (!gone && answer) updateSession(handle, (old) => (old ? { ...old, folders: answer.folders } : old))
    }, () => {
      // A chat the agent has no folders for, such as one it no longer has open, keeps the list it
      // had; the tree then shows what the main process last let it read.
    })
    return () => { gone = true }
  }, [handle, updateSession])

  const addFolder = useCallback(async (from: AskedFrom) => {
    const session = handleRef.current
    if (!session) return
    let added
    try {
      added = folderAdded(await window.bravebot.addFolder(session))
    } catch (error) {
      added = { kind: 'refused' as const, message: String(error) }
    }
    if (added.kind === 'cancelled') return
    if (added.kind === 'refused') { refuse(session, from, added.message); return }
    take(session, added.answer)
    showToast(t.uiFoldersAdded(), added.folder ?? undefined)
  }, [handleRef, refuse, take])

  const closeFolder = useCallback(async (path: string) => {
    const session = handleRef.current
    if (!session) return
    try {
      const answer = foldersAnswer(await call('session.folders.close', { session, path }))
      if (!answer) { refuse(session, 'panel', t.uiFoldersNotClosed()); return }
      take(session, answer)
      showToast(t.uiFoldersClosed(), path)
    } catch (error) {
      refuse(session, 'panel', error instanceof RequestFailed ? error.reason : String(error))
    }
  }, [handleRef, refuse, take])

  return { addFolder, closeFolder }
}
