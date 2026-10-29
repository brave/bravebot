import { useCallback, useEffect, useRef, type MutableRefObject, type SetStateAction } from 'react'
import type { FileAttachment } from '../shared/files'
import type { AskAnswer } from '../shared/protocol'
import { canSend } from './can-send'
import { declined, markersOf, mayGo, route, sentInto } from './interjecting'
import { type Interjection, type Live, NEW_CHAT, liveKey } from './live'
import { conversationPreferences, setConversation } from './experience'
import { Unconfigurable, METHOD, call, callBot, callSummon } from './rpc'
import { grantsOf, named, sent, summonOf, type Staged } from './staging'
import * as t from './transcript'

/** What `useTurn` takes from the component that owns the sessions and the composer. */
export interface TurnInputs {
  liveRef: MutableRefObject<Live | null>
  handleRef: MutableRefObject<string | null>
  openedLives: MutableRefObject<Map<string, Live>>
  updateSession: (handle: string, action: SetStateAction<Live | null>) => void
  setLive: (action: SetStateAction<Live | null>) => void
  setProblem: (problem: string | null) => void
  setUnconfigured: (message: string | null) => void
  refreshLives: (action: SetStateAction<number>) => void
  live: Live | null
  livesRevision: number
  backendReady: boolean | null
  draft: string
  setDraft: (text: string) => void
}

/** Sending, queueing, interjecting and cancelling turns, and answering what a turn asks. */
export function useTurn({ handleRef, openedLives, updateSession, setLive, setProblem, setUnconfigured, refreshLives, live, livesRevision, backendReady, draft, setDraft }: TurnInputs) {
  const answeringTrust = useRef(false)

  const answerTrust = useCallback(async (trusted: boolean) => {
    const handle = handleRef.current
    // One answer per question: a second click would be refused, the offer to keep a yes being spent.
    if (!handle || answeringTrust.current) return
    answeringTrust.current = true
    const path = openedLives.current.get(handle)?.keepingTrust ?? null
    const shownPath = openedLives.current.get(handle)?.shownKeepingTrust ?? null
    // Said back to the bridge only for a question that showed the sentence: the bridge takes the
    // answer to cover confined lines (RUN-5) from this and from nothing else.
    const confinedLines = !!openedLives.current.get(handle)?.confinedLinesNotice
    try {
      const { kept } = await call<{ trusted: boolean; kept: boolean | null }>('trust.reply', { session: handle, trusted, confinedLines })
      const remembered = kept === true && path && shownPath ? { at: Math.floor(Date.now() / 1000), path, shownPath } : null
      updateSession(handle, (old) => (old ? {
        ...old,
        askingTrust: null,
        keepingTrust: null,
        shownKeepingTrust: null,
        confinedLinesNotice: null,
        trustRemembered: remembered ?? old.trustRemembered,
      } : old))
      // Still a yes, for this session. Said, because the next session here will ask after all.
      if (kept === false) {
        setProblem(`Trusting this directory for this chat only: the answer could not be written to ${shownPath ?? path}, so the next chat here will ask.`)
      }
    } catch (error) {
      setProblem(String(error))
    } finally {
      answeringTrust.current = false
    }
  }, [])

  const send = useCallback(async (prompt: string, target?: string, selectedFiles?: FileAttachment[], queuedStaged?: Staged[]) => {
    const handle = target ?? handleRef.current
    if (!handle) return
    // Read before the state below is changed, because that is what clears it: this is the turn
    // that carries the briefing, and by the time it has been sent the session is grounded again.
    const sending = openedLives.current.get(handle)
    const bot = sending?.bot ?? null
    const model = sending?.model ?? null
    const attachments = selectedFiles ?? sending?.attachments ?? []
    // A queued message carries what it named when it was queued. Otherwise it is what the draft
    // still names: a marker deleted before sending took its file or picture off.
    const staged = queuedStaged ?? named(sending?.staging, prompt)
    const grants = grantsOf(staged)
    // The bot picked from the `@` list for this message, which answers this turn alone (MEMORY-14).
    const summoned = summonOf(staged)
    // The files the prompt names with `@`, asked of the bridge before anything is drawn: a name
    // that cannot go stops the send here, with the message still where it was written. The bridge
    // reads the prompt again at `turn.send`, and that check is the one that decides what goes.
    const mentioned = await window.bravebot.request<{ files: string[] }>('mentions.named', { session: handle, prompt })
    if (!mentioned.ok) {
      const refused = mentioned.error?.message ?? 'The files this message names could not be checked.'
      const queued = selectedFiles !== undefined
      updateSession(handle, (old) => old ? {
        ...old,
        sendRefused: refused,
        queued: queued ? [{ prompt, attachments, staged }, ...(old.queued ?? [])] : old.queued,
        queuePaused: queued ? true : old.queuePaused,
      } : old)
      if (!queued && sending) {
        const key = liveKey(sending)
        if (!conversationPreferences(key).draft.trim()) setConversation(key, { draft: prompt })
      }
      return
    }
    const mentionedFiles = Array.isArray(mentioned.ok.files) ? mentioned.ok.files : []
    const reads = [...new Set([...attachments.map((file) => file.path), ...mentionedFiles])]
    // Made here rather than inside the update so its id can be remembered: the agent says where
    // this prompt landed when the turn ends, and that answer has to find the row it belongs to.
    const said = t.userSaid(prompt, summoned?.slug)
    updateSession(handle, (old) =>
      old
        ? {
            ...old,
            summary: old.summary.title === NEW_CHAT ? { ...old.summary, title: prompt.slice(0, 70) } : old.summary,
            attachments: selectedFiles ? old.attachments : [],
            sendRefused: null,
            staging: queuedStaged ? old.staging : sent(old.staging),
            entries: [
              ...old.entries,
              ...reads.map((path): t.Entry => ({ kind: 'attached', id: crypto.randomUUID(), path })),
              // A dropped text file is read at the top of the turn as a picked one is. A picture or
              // a PDF goes in the prompt's own message, whose marker already says so.
              ...staged.flatMap((item): t.Entry[] => item.via === 'drop' && item.file.kind === 'text' ? [{ kind: 'attached', id: crypto.randomUUID(), path: item.file.name }] : []),
              said,
            ],
            awaitingOrdinal: said.id,
            running: true,
            queuePaused: old.queued?.length ? old.queuePaused : false,
            // A summoned turn carries no briefing of this conversation's bot.
            bot: old.bot ? { ...old.bot, grounded: summoned ? old.bot.grounded : true } : null,
          }
        : old,
    )
    try {
      if (summoned) {
        // The main process names the summoned bot's definition and sends it for this turn alone.
        await callSummon({ session: handle, slug: summoned.slug, prompt, model, attachments: attachments.map((file) => file.id), ...grants })
      } else if (bot) {
        // A bot's turn never goes through `call`. It needs files attached, and the main process
        // strips those from anything a window sends — a window that could name a file to read
        // would be a window that could have the planner read any file on the machine. So this
        // names the bot and says whether the briefing is due, and the paths are composed over
        // there from a definition this side cannot reach.
        await callBot({ session: handle, slug: bot.slug, prompt, grounded: !bot.grounded, model, attachments: attachments.map((file) => file.id), ...grants })
      } else {
        await call('turn.send', { session: handle, prompt, model, attachments: attachments.map((file) => file.id), ...grants })
      }
    } catch (error) {
      if (error instanceof Unconfigurable) {
        setUnconfigured(error.message)
        updateSession(handle, (old) => (old ? { ...old, running: false, queuePaused: true, awaitingOrdinal: null, entries: [...old.entries, t.errored(error.message)] } : old))
        return
      }
      updateSession(handle, (old) =>
        old
          ? {
              ...old,
              entries: [...old.entries, t.errored(String(error))],
              queuePaused: true,
              running: false,
              // Nothing was sent, so this prompt is in no conversation and there is no ordinal
              // coming for it. Left standing, it would take the ordinal of the next turn to
              // finish, which may be one nobody in this window asked for.
              awaitingOrdinal: null,
              // Put back. Nothing was sent, so nothing was said — a briefing marked delivered by a
              // turn that failed would be one the bot never received.
              bot: old.bot ? { ...old.bot, grounded: bot?.grounded ?? false } : null,
            }
          : old,
      )
    }
  }, [])

  // Chats whose queued message is between leaving the queue and `send` returning. `send` awaits the
  // bridge's check of the message's `@` names before it marks the chat running, and without this the
  // queue would take the next message in that gap and send both.
  const dequeuing = useRef(new Set<string>())
  useEffect(() => {
    for (const item of openedLives.current.values()) {
      if (item.running || !item.queued?.length || item.askingTrust || !mayGo(item)) continue
      if (dequeuing.current.has(item.handle)) continue
      const [message, ...remaining] = item.queued
      updateSession(item.handle, (old) => old ? { ...old, queued: remaining } : old)
      if (!message) continue
      const handle = item.handle
      dequeuing.current.add(handle)
      void send(message.prompt, handle, message.attachments, message.staged).finally(() => {
        dequeuing.current.delete(handle)
        refreshLives((revision) => revision + 1)
      })
    }
  }, [live, livesRevision, send, updateSession])

  /**
   * Send a line into the turn that is running (INPUT-10), or say it was not taken.
   *
   * The line is drawn as sent before the request goes out, because the turn can read it and say so
   * before the answer to this call arrives, and an event with nothing to match would be lost. A line
   * the bridge does not accept is one nothing will read: no turn is running, it is being stopped, or
   * it is a plan run. It becomes a turn of its own.
   */
  const interject = useCallback(async (handle: string, line: Interjection) => {
    const accepted = await call<{ accepted: boolean }>('turn.interject', { session: handle, prompt: line.prompt, ...markersOf(line.staged) })
      .then((answer) => answer.accepted === true, () => false)
    if (!accepted) {
      updateSession(handle, (old) => old ? declined(old, line.id) : old)
      refreshLives((revision) => revision + 1)
    }
  }, [updateSession])

  const cancel = useCallback(async () => {
    const handle = handleRef.current
    if (handle) {
      updateSession(handle, (old) => old ? { ...old, queuePaused: true } : old)
      await call('turn.cancel', { session: handle }).catch((error) => setProblem(String(error)))
    }
  }, [])

  /**
   * Answer whichever question is on screen.
   *
   * One callback for every kind in [`t.REPLY`], because the shape of the exchange is identical
   * and the differences are entirely in which method carries it. `remember` is only ever true
   * for a run: it is the second answer that question has and the others do not.
   *
   * The card is only marked once the agent has accepted the answer. Marking it first would
   * draw an approval the turn never received if the call failed, which is the one direction
   * this must not be wrong in.
   */
  const answer = useCallback(
    async (kind: t.Asked, request: number, approve: boolean, remember = false) => {
      const handle = handleRef.current
      if (!handle) return
      const decision = approve ? 'approve' : 'reject'
      try {
        await call(METHOD[kind], { session: handle, request, decision, remember })
        updateSession(handle, (old) =>
          old
            ? { ...old, entries: t.decide(old.entries, kind, request, decision, remember) }
            : old,
        )
      } catch (error) {
        setProblem(String(error))
      }
    },
    [],
  )

  /**
   * Answer a series of questions.
   *
   * Separate from [`answer`] because this reply is not a decision: it carries one answer per
   * question, and there is no approve/reject for it to be a flavour of.
   */
  const answerQuestions = useCallback(async (request: number, answers: AskAnswer[]) => {
    const handle = handleRef.current
    if (!handle) return
    try {
      await call('ask.reply', { session: handle, request, answers })
      updateSession(handle, (old) => (old ? { ...old, entries: t.answered(old.entries, request, answers) } : old))
    } catch (error) {
      setProblem(String(error))
    }
  }, [])

  /**
   * Start a manifest run for a task: plan all of it, ask once, then walk the plan.
   *
   * One run per press. The session does not hold this as a mode, so the next message is an
   * ordinary turn unless the button is pressed again.
   */
  const plan = useCallback(async (task: string) => {
    const handle = handleRef.current
    if (!handle) return
    const model = openedLives.current.get(handle)?.model ?? null
    updateSession(handle, (old) => old ? { ...old, running: true, entries: [...old.entries, t.planAsked(task)] } : old)
    try {
      await call('manifest.run', { session: handle, task, model })
    } catch (error) {
      if (error instanceof Unconfigurable) setUnconfigured(error.message)
      updateSession(handle, (old) => old ? { ...old, running: false, queuePaused: true, entries: [...old.entries, t.errored(error instanceof Unconfigurable ? error.message : String(error))] } : old)
    }
  }, [])

  /** Start a run from whatever is in the composer, where a run can take it. */
  const submitPlan = useCallback(() => {
    const task = draft.trim()
    if (!handleRef.current || !canSend(live, backendReady, task)) return
    // A run reads nothing before it plans, so it cannot take attached files, and a bot's turn
    // carries a briefing a run has no place for.
    if (live?.attachments?.length || named(live?.staging, task).length || live?.bot) return
    setDraft('')
    void plan(task)
  }, [draft, live?.running, live?.askingTrust, live?.attachments, live?.staging, live?.bot, plan, backendReady])

  /** Hold the draft for after the turn: each queued message starts a turn of its own, with the grants it names. */
  const queueDraft = () => {
    if (!draft.trim()) return
    setLive((old) => old ? {
      ...old,
      queued: [...(old.queued ?? []), { prompt: draft.trim(), attachments: old.attachments ?? [], staged: named(old.staging, draft) }],
      attachments: [],
      staging: sent(old.staging),
    } : old)
    setDraft('')
  }

  /** Send whatever is in the composer, on the same terms the Send button uses. */
  const submit = useCallback(() => {
    const prompt = draft.trim()
    if (!handleRef.current || !canSend(live, backendReady, prompt)) return
    setDraft('')
    void send(prompt)
  }, [draft, live?.running, live?.askingTrust, send, backendReady])

  /** What the composer's interject key does: queue the line behind the turn, or send it into the turn. */
  const interjectDraft = () => {
    const handle = handleRef.current
    const text = draft.trim()
    if (!handle || !text || !live) return
    if (route(live, named(live.staging, draft)) === 'queue') { queueDraft(); return }
    const staged = named(live.staging, draft)
    const line = { id: crypto.randomUUID(), prompt: text, attachments: [], staged }
    updateSession(handle, (old) => old ? sentInto(old, line) : old)
    setDraft('')
    void interject(handle, line)
  }

  return { answerTrust, submit, submitPlan, queueDraft, interject, cancel, answer, answerQuestions, interjectDraft }
}
