/**
 * The desktop as a client of the host, drawn from the Rust view (HOSTCLIENT-2).
 *
 * The state is whatever the main process holds of the shared client, and every question, row and
 * name below comes from it. This window sends the message a person typed and nothing it composed
 * itself: a message another client sent appears here from the view alone, and a send the host
 * refuses leaves the draft where it was written. It offers only what the host lets a client do, so
 * no control here answers a kind this client may not answer or touches a setting.
 */

import { Component, useEffect, useMemo, useRef, useState, type ErrorInfo, type ReactNode } from 'react'
import type { AskAnswer } from '../../shared/protocol'
import type { AskAnswer as SentAnswer, PrototypeAction, PrototypeOutcome, PrototypeSession, PrototypeState } from '../../shared/prototype'
import { TrustPrompt } from '../components/TrustPrompt'
import { Row } from '../components/Transcript'
import { Button, Icon } from '../nala'
import { ENDED, entriesFromRows, SETTLED, type AnsweredHere } from './entries'
import { withAnswerOut, withDraft, withSending } from './keyed'

export interface PrototypeApi {
  state(): Promise<PrototypeState | null>
  subscribe(listener: (state: PrototypeState) => void): () => void
  act(action: PrototypeAction): Promise<PrototypeOutcome>
}

/** What the host's code for a refused action says, in words for the person. */
export function refusal(outcome: Extract<PrototypeOutcome, { ok: false }>): string {
  switch (outcome.code) {
    case 'turn_in_flight':
      return 'A turn is already running in this session, so this was not sent. It stays here until you send it again.'
    case 'not_allowed':
      return 'The host does not let this window do that.'
    case 'no_such_request':
      return 'That question is not waiting any more: another client answered it first.'
    case 'no_such_session':
      return 'The host no longer has that session.'
    default:
      return outcome.message
  }
}

/** What the question card gave, in the one shape the host's client takes: chosen, typed, or declined. */
export function sentAnswer(answer: AskAnswer): SentAnswer {
  if (Array.isArray(answer.chosen)) return { chosen: answer.chosen }
  if (typeof answer.typed === 'string') return { typed: answer.typed }
  return null
}

/** What a question this window cannot answer says became of it. */
function noteFor(row: PrototypeSession['view']['rows'][number] | undefined, outstanding: ReadonlySet<number>, ended: boolean): string {
  if (!row?.resolved) return ended ? ENDED : 'Answer this on another device'
  const request = typeof row.data === 'object' && row.data !== null && !Array.isArray(row.data) ? row.data.request : null
  return typeof request === 'number' && outstanding.has(request) ? 'Your answer was sent. Waiting for the host to say whether it was taken' : SETTLED
}

/** Whether the turn on screen can be cancelled. */
const live = (status: string): boolean => status === 'running' || status === 'waiting'

const STATUS: Record<string, string> = {
  awaiting_trust: 'Waiting for the folder to be trusted',
  idle: 'Ready',
  running: 'Working',
  waiting: 'Waiting for an answer',
  completed: 'Done',
  failed: 'Stopped by an error',
  cancelled: 'Cancelled',
  detached: 'Closed',
}

/** One row drawn, and a line saying so if drawing it fails, so one bad payload does not blank the window. */
class DrawnRow extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false }
  static getDerivedStateFromError(): { failed: boolean } {
    return { failed: true }
  }
  componentDidCatch(_error: Error, _info: ErrorInfo): void {}
  render(): ReactNode {
    return this.state.failed ? <div className="narration"><span>A row could not be drawn.</span></div> : this.props.children
  }
}

/**
 * The startup question to put to the person, if this client may answer it and the view says it is
 * still open. Another client may have answered it, which only the view's status says.
 */
export function trustOf(state: PrototypeState, session: PrototypeSession): { directory: string; keeping: string | null; confinedLines: string | null } | null {
  const asked = session.startupTrust
  if (session.view.status !== 'awaiting_trust' || typeof asked !== 'object' || asked === null || !state.answers.includes('trust')) return null
  const data = asked as Record<string, unknown>
  const text = (value: unknown): string | null => (typeof value === 'string' ? value : null)
  const directory = text(data.shownDirectory) ?? text(data.directory)
  // The sentence about what a yes also grants (RUN-5) is not drawn: the host's client cannot say it
  // was shown, so the host takes a yes from it as not covering confined lines, and the dialog
  // must not say it does.
  return directory === null ? null : { directory, keeping: text(data.shownKeeping), confinedLines: null }
}

interface Answered {
  /** What this window answered in the session on screen, which the host took. */
  here: ReadonlyMap<number, AnsweredHere>
  /** Record an answer once the host has taken it, and not before. */
  record: (request: number, answer: AnsweredHere) => void
}

/** What this window has sent to the host and not heard back about, for one session. */
interface InFlight {
  /** A message is out. */
  sending: boolean
  setSending: (on: boolean) => void
  /** An answer to the startup question is out. */
  trusting: boolean
  setTrusting: (on: boolean) => void
  /** Answers out, by question number. A question that resolves while one is out is not said to have been settled elsewhere: this window does not know yet. */
  answers: ReadonlySet<number>
  setAnswer: (request: number, on: boolean) => void
}

function SessionView({ state, session, api, answered, draft, setDraft, inFlight }: {
  state: PrototypeState
  session: PrototypeSession
  api: PrototypeApi
  answered: Answered
  draft: string
  setDraft: (change: (now: string) => string) => void
  inFlight: InFlight
}): React.JSX.Element {
  const [notice, setNotice] = useState<string | null>(null)
  const { sending, setSending, trusting, setTrusting, answers: outstanding, setAnswer: outstandingFor } = inFlight
  const end = useRef<HTMLDivElement>(null)
  const view = session.view

  const entries = useMemo(
    () => entriesFromRows(view.rows, { here: answered.here, answers: state.answers, ended: view.ended !== null }),
    [view.rows, view.sequence, view.ended, state.answers, answered.here],
  )
  useEffect(() => {
    end.current?.scrollIntoView({ block: 'end' })
  }, [view.sequence])

  const act = async (action: PrototypeAction): Promise<boolean> => {
    const outcome = await api.act(action)
    setNotice(outcome.ok ? null : refusal(outcome))
    return outcome.ok
  }
  const send = async (): Promise<void> => {
    if (sending || draft.trim() === '') return
    setSending(true)
    // The draft is kept until the host has taken the message, so a refusal leaves it as it was.
    const text = draft
    const sent = await act({ kind: 'send', session: session.id, text })
    // Only what was sent is cleared: what was typed while the host took it is still the person's.
    if (sent) setDraft((now) => (now === text ? '' : now))
    setSending(false)
  }

  const awaitingTrust = view.status === 'awaiting_trust'
  const trust = trustOf(state, session)
  const ended = view.ended !== null
  return (
    <section className="prototype-session" data-test="prototype-session">
      <header className="prototype-head">
        <span className="prototype-status" role="status" data-test="prototype-status">{STATUS[view.status] ?? view.status}</span>
        <span className="prototype-actor" data-test="prototype-actor">
          {view.actor === null ? 'No one has acted yet' : `Last action: ${view.actor === state.name ? `${view.actor} (this window)` : view.actor}`}
        </span>
        {live(view.status) && !ended && (
          <Button kind="plain-faint" size="small" onClick={() => void act({ kind: 'cancel', session: session.id })} data-test="prototype-cancel">Cancel</Button>
        )}
        <Button kind="plain-faint" size="small" onClick={() => void act({ kind: 'leave' })} data-test="prototype-leave">Close this view</Button>
      </header>
      {view.omitted > 0 && <p className="prototype-omitted">{view.omitted} earlier items are no longer kept, so they are not shown.</p>}
      <main className="transcript prototype-transcript" data-test="prototype-transcript">
        {entries.map((entry, at) => (
          <DrawnRow key={entry.id}>
            <Row
              entry={entry}
              forkable={false}
              onFork={() => {}}
              onDecide={(kind, request, approve) => {
                if (kind === 'confirm' || kind === 'run' || kind === 'fetch') {
                  const decision = approve ? 'approve' : 'reject'
                  // Said to be this window's answer only once the host has taken it: a question
                  // another client answered first is refused, and is not this window's.
                  if (outstanding.has(request)) return
                  outstandingFor(request, true)
                  void act({ kind: 'decide', session: session.id, request, decision }).then((taken) => {
                    outstandingFor(request, false)
                    if (taken) answered.record(request, decision)
                  })
                }
              }}
              onAnswer={(request, given) => {
                const sent = given.map(sentAnswer)
                // A second click on a question already answered is not sent again.
                if (outstanding.has(request)) return
                outstandingFor(request, true)
                void act({ kind: 'answer', session: session.id, request, answers: sent }).then((taken) => {
                  outstandingFor(request, false)
                  if (taken) answered.record(request, given)
                })
              }}
              interruptedNote={noteFor(view.rows[at], outstanding, view.ended !== null)}
            />
          </DrawnRow>
        ))}
        <div ref={end} />
      </main>
      {trust && !ended && (
        <TrustPrompt directory={trust.directory} keeping={trust.keeping} confinedLines={trust.confinedLines} onAnswer={(trusted) => {
          // The dialog stays until the view says the question is closed, and a second answer is not sent.
          if (trusting) return
          setTrusting(true)
          void act({ kind: 'trust', session: session.id, trusted }).then(() => setTrusting(false))
        }} />
      )}
      <footer className="composer prototype-composer">
        {ended && <p className="prototype-note" role="status" data-test="prototype-ended">This session's view has ended{view.ended?.detail ? `: ${view.ended.detail}` : ''}.</p>}
        {awaitingTrust && !trust && !ended && (
          <p className="prototype-note" role="status" data-test="prototype-trust-elsewhere">This folder is not trusted yet. The host has not let this client answer the question, so another client the host trusts with it answers it.</p>
        )}
        {notice && <p className="prototype-note refused" role="alert" data-test="prototype-notice">{notice}</p>}
        <div className="prototype-box">
          <textarea
            className="prototype-input"
            aria-label="Message the agent"
            value={draft}
            disabled={ended}
            placeholder="Message the agent"
            onChange={(event) => setDraft(() => event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
                event.preventDefault()
                void send()
              }
            }}
          />
          <Button kind="filled" size="small" isDisabled={ended || sending || draft.trim() === ''} onClick={() => void send()} data-test="prototype-send">Send</Button>
        </div>
      </footer>
    </section>
  )
}

export function PrototypeApp({ api }: { api: PrototypeApi }): React.JSX.Element {
  const [state, setState] = useState<PrototypeState | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  // What this window answered, by session and question number, kept above the session on screen so
  // closing the view and opening it again does not forget it.
  const [answeredIn, setAnsweredIn] = useState<ReadonlyMap<string, ReadonlyMap<number, AnsweredHere>>>(new Map())
  // What was typed and not sent, by session, so another session's view and back keeps it.
  const [drafts, setDrafts] = useState<ReadonlyMap<string, string>>(new Map())
  // Sends and answers out, by session, kept here for the same reason: leaving a session's view and
  // coming back must not forget that one is out.
  const [sendingIn, setSendingIn] = useState<ReadonlySet<string>>(new Set())
  const [trustingIn, setTrustingIn] = useState<ReadonlySet<string>>(new Set())
  const [answersOut, setAnswersOut] = useState<ReadonlyMap<string, ReadonlySet<number>>>(new Map())
  useEffect(() => {
    let current = true
    void api.state().then((first) => {
      if (current && first) setState((old) => old ?? first)
    })
    const stop = api.subscribe((next) => setState(next))
    return () => {
      current = false
      stop()
    }
  }, [api])

  if (state === null) return <div className="prototype" data-test="prototype"><p className="prototype-note">Connecting to the host…</p></div>
  const act = async (action: PrototypeAction): Promise<void> => {
    const outcome = await api.act(action)
    setNotice(outcome.ok ? null : refusal(outcome))
  }
  return (
    <div className="prototype" data-test="prototype">
      <aside className="prototype-sessions" aria-label="Sessions">
        <h1 className="prototype-title" data-test="prototype-name">{state.name}</h1>
        <p className="prototype-sub">{state.workspace}</p>
        {state.channel === 'ended' && (
          <p className="prototype-note refused" role="alert" data-test="prototype-channel-ended">The host ended the connection{state.endedReason ? `: ${state.endedReason}` : ''}. Sessions it held keep running there only while it does.</p>
        )}
        <Button kind="filled" size="small" isDisabled={state.channel === 'ended'} onClick={() => void act({ kind: 'new' })} data-test="prototype-new">New session</Button>
        <ul className="prototype-list" data-test="prototype-list">
          {state.held.map((held) => (
            <li key={held.session}>
              <button
                type="button"
                className={state.session?.id === held.session ? 'selected' : ''}
                aria-current={state.session?.id === held.session ? 'true' : undefined}
                disabled={state.channel === 'ended' || held.state !== 'open'}
                onClick={() => void act({ kind: 'attach', session: held.session })}
                data-test="prototype-held"
              >
                <Icon name={held.working && state.channel === 'open' ? 'loading-spinner' : 'message-bubble-chat'} />
                <span className="prototype-id">{held.session}</span>
                <span className="prototype-meta">{held.state}{held.working && state.channel === 'open' ? ' · working' : ''}{held.attached ? ' · here' : ''}</span>
              </button>
            </li>
          ))}
          {state.held.length === 0 && <li className="prototype-empty">The host holds no sessions.</li>}
        </ul>
        {notice && <p className="prototype-note refused" role="alert" data-test="prototype-list-notice">{notice}</p>}
      </aside>
      {state.session
        ? <SessionView
            key={state.session.id}
            state={state}
            session={state.session}
            api={api}
            inFlight={{
              sending: sendingIn.has(state.session.id),
              setSending: (on) => {
                const id = state.session!.id
                setSendingIn((all) => withSending(all, id, on))
              },
              trusting: trustingIn.has(state.session.id),
              setTrusting: (on) => {
                const id = state.session!.id
                setTrustingIn((all) => withSending(all, id, on))
              },
              answers: answersOut.get(state.session.id) ?? new Set(),
              setAnswer: (request, on) => {
                const id = state.session!.id
                setAnswersOut((all) => withAnswerOut(all, id, request, on))
              },
            }}
            draft={drafts.get(state.session.id) ?? ''}
            setDraft={(change) => {
              const id = state.session!.id
              setDrafts((all) => withDraft(all, id, change))
            }}
            answered={{
              here: answeredIn.get(state.session.id) ?? new Map(),
              record: (request, answer) => {
                const id = state.session!.id
                setAnsweredIn((all) => new Map(all).set(id, new Map(all.get(id) ?? []).set(request, answer)))
              },
            }}
          />
        : <section className="prototype-session empty" data-test="prototype-empty"><p className="prototype-note">Open a session from the list, or start a new one. Closing a view does not close the session: the host keeps it and its work.</p></section>}
    </div>
  )
}
