import { useCallback, useEffect, useRef, type MutableRefObject, type SetStateAction } from 'react'
import type { FileAttachment } from '../shared/files'
import type { AskAnswer, ProjectQuestionsReport } from '../shared/protocol'

/** What `trust.reply` answers: whether the yes was kept, and the questions it made due. */
type TrustReplied = ProjectQuestionsReport & { trusted: boolean; kept: boolean | null }
import { askingFirst, canSend } from './can-send'
import { type Answering, replyFolder, replyRules, reported } from './project-questions'
import { declined, markersOf, mayGo, route, sentInto } from './interjecting'
import { type Interjection, type Live, NEW_CHAT, liveKey } from './live'
import { conversationPreferences, setConversation } from './experience'
import { Unconfigurable, METHOD, call, callBot, callSummon } from './rpc'
import { RequestFailed } from './rewind'
import type { ScheduleAnswer } from '../shared/schedule'
import type { AsideStarted } from '../shared/aside'
import { manifestTask } from '../shared/manifest'
import { planRun as startPlanRun } from './plan-run'
import { planTask } from '../shared/plan'
import { carryOutPlan } from './plan-line'
import { exitOf, folded, grantsOf, named, sent, summonOf, type Staged } from './staging'
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
      const settled = await call<TrustReplied>('trust.reply', { session: handle, trusted, confinedLines })
      const kept = settled.kept
      const remembered = kept === true && path && shownPath ? { at: Math.floor(Date.now() / 1000), path, shownPath } : null
      // A yes may put the question about the rules this checkout proposes, or the first about a
      // directory it names: the answer says which is out, and the window draws that next.
      const next = reported(settled)
      updateSession(handle, (old) => (old ? {
        ...old,
        ...next,
        rules: next.rules !== undefined ? next.rules : old.rules,
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

  // The questions after trust, answered the same way: once per question, by its request id, and
  // what comes next read from the answer. The bridge grants and opens; this only says yes or no.
  const answeringProject = useRef(false)
  const answering: Answering = {
    call,
    read: (handle) => openedLives.current.get(handle),
    update: (handle, change) => updateSession(handle, (old) => (old ? { ...old, ...change } : old)),
    setProblem,
  }
  const answerProject = useCallback(async (reply: (handle: string) => Promise<void>) => {
    const handle = handleRef.current
    if (!handle || answeringProject.current) return
    answeringProject.current = true
    try {
      await reply(handle)
    } finally {
      answeringProject.current = false
    }
  }, [])
  const answerRules = useCallback((granted: boolean) => answerProject((handle) => replyRules(answering, handle, granted)), [])
  const answerFolder = useCallback((open: boolean) => answerProject((handle) => replyFolder(answering, handle, open)), [])

  const send = useCallback(async (prompt: string, target?: string, selectedFiles?: FileAttachment[], queuedStaged?: Staged[]) => {
    const handle = target ?? handleRef.current
    if (!handle) return
    // Read before the state below is changed, because that is what clears it: this is the turn
    // that carries the briefing, and by the time it has been sent the session is grounded again.
    const sending = openedLives.current.get(handle)
    const bot = sending?.bot ?? null
    const model = sending?.model ?? null
    const attachments = selectedFiles ?? sending?.attachments ?? []
    // A queued message carries what it named when it was queued, and a draft from the composer
    // what it named before its folded pastes were put back. Otherwise it is what the prompt still
    // names: a marker deleted before sending took its file or picture off.
    const staged = queuedStaged ?? named(sending?.staging, prompt)
    // A queued message is sent with the files it was queued with and takes nothing off the
    // composer. A draft from the composer leaves with what the composer held.
    const queued = selectedFiles !== undefined
    const grants = grantsOf(staged)
    // The bot picked from the `@` list for this message, which answers this turn alone (MEMORY-14).
    const summoned = summonOf(staged)
    // The line as the box showed it, each folded paste behind its marker. The names are read there
    // and not in the pasted words, which name no file (NAME-12). Sent only when it differs.
    const line = folded(staged, prompt)
    const shown = line === prompt ? {} : { shown: line }
    // The files the prompt names with `@`, asked of the bridge before anything is drawn: a name
    // that cannot go stops the send here, with the message still where it was written. The bridge
    // reads the names again at `turn.send`, and that check is the one that decides what goes.
    const mentioned = await window.bravebot.request<{ files: string[] }>('mentions.named', { session: handle, prompt: line })
    if (!mentioned.ok) {
      const refused = mentioned.error?.message ?? 'The files this message names could not be checked.'
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
            staging: queued ? old.staging : sent(old.staging),
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
        await callSummon({ session: handle, slug: summoned.slug, prompt, ...shown, model, attachments: attachments.map((file) => file.id), ...grants })
      } else if (bot) {
        // A bot's turn never goes through `call`. It needs files attached, and the main process
        // strips those from anything a window sends — a window that could name a file to read
        // would be a window that could have the planner read any file on the machine. So this
        // names the bot and says whether the briefing is due, and the paths are composed over
        // there from a definition this side cannot reach.
        await callBot({ session: handle, slug: bot.slug, prompt, ...shown, grounded: !bot.grounded, model, attachments: attachments.map((file) => file.id), ...grants })
      } else {
        // A new id for each Send, which the agent requires on every send (RPCVIEW-10).
        await call('turn.send', { session: handle, prompt, ...shown, model, messageId: crypto.randomUUID(), attachments: attachments.map((file) => file.id), ...grants })
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

  /**
   * Carry out `/schedule <argument>` for a line a person typed into the composer (CMD-1).
   *
   * The bridge cuts the line and runs the code `bravebot schedule` runs. What it answers is drawn as
   * a row of the transcript and is never sent to the model. A form that changes the schedule is
   * refused by the bridge while a turn runs (`turn_in_flight`, the bridge's decision and not a
   * reading of the line here), and the line then waits in the queue to be asked again when the
   * turn ends (CMD-8). Nothing here reads the argument. A line typed while a `/schedule` line already
   * waits waits behind it, whatever it is, since the bridge cannot see this queue and would answer a
   * read before the write typed first (CMD-8).
   *
   * `staged` is what the line names. Only its markers and file names are sent, and the bridge puts
   * each marker back into the argument as the file's name, or as the sentence that a picture cannot
   * join a command (DROP-4). A line that waits keeps them for when it is asked again.
   */
  const schedule = useCallback(async (handle: string, argument: string, line: string, staged: Staged[], queuedAlready = false) => {
    const waiting = !queuedAlready && openedLives.current.get(handle)?.queued?.some((item) => item.schedule !== undefined)
    if (waiting) {
      updateSession(handle, (old) => old ? { ...old, queued: [...(old.queued ?? []), { prompt: line, attachments: [], staged, schedule: argument }] } : old)
      return
    }
    try {
      const answer = await call<ScheduleAnswer>('schedule.command', { session: handle, argument, ...markersOf(staged) })
      updateSession(handle, (old) => old ? { ...old, entries: [...old.entries, t.scheduleAnswered(answer)] } : old)
    } catch (error) {
      if (error instanceof RequestFailed && error.code === 'turn_in_flight' && !queuedAlready) {
        updateSession(handle, (old) => old ? { ...old, queued: [...(old.queued ?? []), { prompt: line, attachments: [], staged, schedule: argument }] } : old)
        return
      }
      updateSession(handle, (old) => old ? { ...old, entries: [...old.entries, t.errored(String(error))] } : old)
    }
  }, [])

  /**
   * Ask a typed `/btw <question>` or `/recap` of the bridge (CMD-1, CMD-8, CMD-16). The line is never
   * sent as a prompt, and its answer arrives as `aside.done`, drawn as a row and never part of the
   * conversation.
   *
   * A `/btw` goes out at once, while a turn runs and lines wait in the queue too (CMD-8): the bridge
   * asks it over the exchange the turn sent. A `/recap` the bridge refuses while a turn runs
   * (`turn_in_flight`, the bridge's decision) waits in the queue and is asked again when the turn
   * ends (CMD-16). A `/btw` carries the pictures and files staged on its line (CMD-5). One the
   * bridge refuses with staged items, such as a dropped file while a turn runs, goes back in the
   * composer with them, so it can be asked again. A file chosen with the paperclip is a grant a
   * question cannot be handed, so a `/btw` with one is refused here.
   */
  const askAside = useCallback(async (handle: string, kind: 'btw' | 'recap', question: string, line: string, onLine: Staged[], queuedAlready = false) => {
    const asking = openedLives.current.get(handle)
    if (kind === 'btw' && (asking?.attachments?.length ?? 0) > 0) {
      if (handleRef.current === handle) setDraft(line)
      setProblem('A /btw question cannot carry files chosen with the paperclip. Take them off and ask again.')
      return
    }
    const model = asking?.model ?? null
    // What was pasted or dropped on the line goes with the question (CMD-5): pictures and PDFs as
    // the ids the main process resolves, as it does for a prompt's, and a text file as its name
    // (DROP-4). The bridge refuses a dropped file while a turn runs, in the terminal's words.
    // `onLine` was found on the line before its folded pastes were put back, and `line` is the
    // words, so a line put back in the composer holds no marker that names nothing.
    const staged = kind === 'btw' ? onLine : []
    const carried = {
      pastes: grantsOf(staged).pastes,
      drops: staged.flatMap((item) => item.via === 'drop' && item.file.kind !== 'text' ? [item.file.id] : []),
      files: markersOf(staged.filter((item) => item.via === 'drop' && item.file.kind === 'text')).files,
    }
    try {
      const started = kind === 'btw'
        ? await call<AsideStarted>('session.btw', { session: handle, question, model, ...carried })
        : await call<AsideStarted>('session.recap', { session: handle, model })
      if (staged.length > 0) updateSession(handle, (old) => old ? { ...old, staging: sent(old.staging) } : old)
      if (started.besideTurn) updateSession(handle, (old) => old ? { ...old, entries: [...old.entries, t.asideOut(question)] } : old)
    } catch (error) {
      if (kind === 'recap' && error instanceof RequestFailed && error.code === 'turn_in_flight' && !queuedAlready) {
        updateSession(handle, (old) => old ? { ...old, queued: [...(old.queued ?? []), { prompt: line, attachments: [], staged: [], aside: 'recap' }] } : old)
        return
      }
      if (staged.length > 0 && handleRef.current === handle) setDraft(line)
      updateSession(handle, (old) => old ? { ...old, entries: [...old.entries, t.errored(error instanceof RequestFailed ? error.reason : String(error))] } : old)
    }
  }, [])

  /** Start a plan run for a typed `/manifest <task>` line (MANIFEST-9). See `plan-run.ts`. */
  const planRun = useCallback((handle: string, task: string, line: string, queuedAlready = false) => startPlanRun({
    call,
    read: (target) => openedLives.current.get(target),
    update: (target, change) => updateSession(target, (old) => old ? change(old) : old),
    putBack: (target, text) => { if (handleRef.current === target) setDraft(text) },
    setProblem,
  }, handle, task, line, queuedAlready), [])

  /**
   * Carry out a typed `/plan [task]` line (MODE-12): queued while a turn runs or another plan line
   * waits, otherwise plan mode is set and then, with a task, a turn is sent the task alone.
   */
  const plan = useCallback((handle: string, task: string, line: string, staged: Staged[], queuedAlready = false) => carryOutPlan({
    call,
    read: (target) => openedLives.current.get(target),
    update: (target, change) => updateSession(target, (old) => (old ? change(old) : old)),
    send,
  }, handle, task, line, staged, queuedAlready), [send, updateSession])

  // Chats whose queued message is between leaving the queue and `send` returning. `send` awaits the
  // bridge's check of the message's `@` names before it marks the chat running, and without this the
  // queue would take the next message in that gap and send both.
  const dequeuing = useRef(new Set<string>())
  useEffect(() => {
    for (const item of openedLives.current.values()) {
      if (item.running || !item.queued?.length || askingFirst(item) || !mayGo(item)) continue
      if (dequeuing.current.has(item.handle)) continue
      const [message, ...remaining] = item.queued
      updateSession(item.handle, (old) => old ? { ...old, queued: remaining } : old)
      if (!message) continue
      const handle = item.handle
      dequeuing.current.add(handle)
      // A waiting `/schedule` or `/recap` line is asked again and starts no turn, a waiting
      // `/manifest` line starts its run, and a waiting `/plan` line sets plan mode and then sends
      // its task alone.
      const settled = message.aside === 'recap'
        ? askAside(handle, 'recap', '', message.prompt, [], true)
        : message.schedule !== undefined
          ? schedule(handle, message.schedule, message.prompt, message.staged, true)
          : message.manifest !== undefined
            ? planRun(handle, message.manifest, message.prompt, true)
            : message.plan !== undefined
              ? plan(handle, message.plan, message.prompt, message.staged, true)
              : send(message.prompt, handle, message.attachments, message.staged)
      void settled.finally(() => {
        dequeuing.current.delete(handle)
        refreshLives((revision) => revision + 1)
      })
    }
  }, [live, livesRevision, send, askAside, planRun, plan, updateSession])

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

  /** Hold the draft for after the turn: each queued message starts a turn of its own, with the grants it names. */
  const queueDraft = () => {
    // Each exit carries the words, with every folded paste put back, and what the folded draft
    // names (PASTE-1).
    const exit = exitOf(live?.staging, draft)
    if (!exit) return
    if (exit.to === 'schedule' && handleRef.current) {
      const handle = handleRef.current
      setDraft('')
      setLive((old) => old ? { ...old, attachments: [], staging: sent(old.staging) } : old)
      void schedule(handle, exit.argument, exit.line, exit.staged)
      return
    }
    if (exit.to === 'aside' && handleRef.current) {
      setDraft('')
      void askAside(handleRef.current, exit.kind, exit.question, exit.line, exit.staged)
      return
    }
    const manifest = manifestTask(draft)
    if (manifest !== null && handleRef.current) {
      setDraft('')
      void planRun(handleRef.current, manifest, draft.trim())
      return
    }
    // A plan line waits as one: it sets plan mode and sends its task alone when it leaves (MODE-12).
    const task = exit.to === 'prompt' ? planTask(exit.line) : null
    if (task !== null) {
      setLive((old) => old ? {
        ...old,
        queued: [...(old.queued ?? []), { prompt: exit.line, attachments: [], staged: exit.staged, plan: task }],
        staging: sent(old.staging),
      } : old)
      setDraft('')
      return
    }
    setLive((old) => old ? {
      ...old,
      queued: [...(old.queued ?? []), { prompt: exit.line, attachments: old.attachments ?? [], staged: exit.staged }],
      attachments: [],
      staging: sent(old.staging),
    } : old)
    setDraft('')
  }

  /** Send whatever is in the composer, on the same terms the Send button uses. */
  const submit = useCallback(() => {
    const exit = exitOf(live?.staging, draft)
    if (!handleRef.current || !exit || !canSend(live, backendReady, exit.line)) return
    if (exit.to === 'schedule') {
      setDraft('')
      setLive((old) => old ? { ...old, attachments: [], staging: sent(old.staging) } : old)
      void schedule(handleRef.current, exit.argument, exit.line, exit.staged)
      return
    }
    const task = exit.to === 'prompt' ? planTask(exit.line) : null
    if (task !== null) {
      setDraft('')
      setLive((old) => old ? { ...old, staging: sent(old.staging) } : old)
      void plan(handleRef.current, task, exit.line, exit.staged)
      return
    }
    if (exit.to === 'aside') {
      setDraft('')
      void askAside(handleRef.current, exit.kind, exit.question, exit.line, exit.staged)
      return
    }
    // `planRun` reads the folded draft, so a paste or other staged item it names is refused.
    const manifest = manifestTask(draft)
    if (manifest !== null) {
      setDraft('')
      void planRun(handleRef.current, manifest, draft.trim())
      return
    }
    setDraft('')
    // A refused send puts `exit.line` back in the composer, which is the words.
    void send(exit.line, undefined, undefined, exit.staged)
  }, [draft, live?.staging, live?.running, live?.askingTrust, live?.rulesQuestion, live?.folderQuestion, send, schedule, askAside, planRun, plan, backendReady])

  /** What the composer's interject key does: queue the line behind the turn, or send it into the turn. */
  const interjectDraft = () => {
    const handle = handleRef.current
    if (!handle || !live) return
    const exit = exitOf(live.staging, draft)
    if (!exit) return
    if (exit.to === 'schedule') {
      setDraft('')
      setLive((old) => old ? { ...old, attachments: [], staging: sent(old.staging) } : old)
      void schedule(handle, exit.argument, exit.line, exit.staged)
      return
    }
    // Never interjected: a plan line waits for the running turn (MODE-12).
    const task = exit.to === 'prompt' ? planTask(exit.line) : null
    if (task !== null) {
      setDraft('')
      setLive((old) => old ? { ...old, staging: sent(old.staging) } : old)
      void plan(handle, task, exit.line, exit.staged)
      return
    }
    if (exit.to === 'aside') {
      setDraft('')
      void askAside(handle, exit.kind, exit.question, exit.line, exit.staged)
      return
    }
    // A `/manifest` line is never sent into a turn: it starts a run, or waits for the turn to end.
    const manifest = manifestTask(draft)
    if (manifest !== null) {
      setDraft('')
      void planRun(handle, manifest, draft.trim())
      return
    }
    if (route(live, exit.staged) === 'queue') { queueDraft(); return }
    const line = { id: crypto.randomUUID(), prompt: exit.line, attachments: [], staged: exit.staged }
    updateSession(handle, (old) => old ? sentInto(old, line) : old)
    setDraft('')
    void interject(handle, line)
  }

  return { answerTrust, answerRules, answerFolder, send, submit, queueDraft, interject, cancel, answer, answerQuestions, interjectDraft, plan }
}
