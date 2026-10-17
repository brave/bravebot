import type { BridgeEvent } from '../shared/protocol'
import * as t from './transcript'
import { receiveTurn } from './turn-details'
import type { Live } from './live'

/** Fold one event into the live session. */
export function apply(
  message: BridgeEvent,
  setLive: React.Dispatch<React.SetStateAction<Live | null>>,
  setBuild: (build: string) => void,
  refresh: () => void,
): void {
  if (message.event === 'agent.ready') {
    setBuild((message.data as { build: string }).build)
    return
  }

  setLive((old) => {
    if (!old) return old
    old = { ...old, turns: receiveTurn(old.turns, message) }
    switch (message.event) {
      case 'watch.fired':
        return { ...old, entries: [...old.entries, t.watchFired(message.data.number, message.data.path)] }
      case 'watch.ended':
        return { ...old, entries: [...old.entries, t.narrated(`Watch ${message.data.number} ended: ${message.data.reason}${message.data.message ? `. ${message.data.message}` : ''}`)] }
      case 'turn.started':
        return { ...old, running: true, phase: null, checking: null, hook: null, composing: null, tokens: 0,
          sandboxMode: message.data.sandbox ?? old.sandboxMode,
          entries: t.beginTurn(old.entries, message.data.turn) }
      case 'audit':
        return old
      // A phase opens a round, so a call written in the last one is over.
      case 'phase':
        return { ...old, phase: message.data.phase, composing: null }
      case 'composing':
        return { ...old, composing: message.data.call }
      // The phase is left alone: it is what the word goes back to once the check is over.
      case 'check.started':
        return { ...old, checking: message.data }
      case 'check.finished':
        return { ...old, checking: null }
      // Likewise left to the phase: a hook runs in the middle of a round.
      case 'hook.started':
        return { ...old, hook: message.data }
      case 'hook.finished':
        return { ...old, hook: null }
      case 'tokens':
        return { ...old, tokens: message.data.written }
      case 'narration':
        return { ...old, composing: null, entries: [...old.entries, t.narrated(message.data.text)] }
      case 'tool.started':
        return { ...old, composing: null, entries: [...old.entries, t.started(message.data)] }
      case 'tool.finished':
        return { ...old, entries: t.finish(old.entries, message.data) }
      case 'landed':
        return { ...old, entries: t.land(old.entries, message.data.landing) }
      case 'quarantined':
        return {
          ...old,
          quarantine: [...old.quarantine, message.data],
          entries: [...old.entries, t.quarantined(message.data)],
        }
      case 'todos':
        return { ...old, todos: message.data.rows }
      case 'confirm.request':
        return { ...old, entries: [...old.entries, t.asked(message.data)] }
      case 'run.request':
        return { ...old, entries: [...old.entries, t.askedRun(message.data)] }
      case 'output.request':
        return { ...old, entries: [...old.entries, t.askedOutput(message.data)] }
      case 'vet.request':
        return { ...old, entries: [...old.entries, t.askedVet(message.data)] }
      case 'fetch.request':
        return { ...old, entries: [...old.entries, t.askedFetch(message.data)] }
      case 'server.request':
        return { ...old, entries: [...old.entries, t.askedServer(message.data)] }
      case 'manifest.request':
        return { ...old, entries: [...old.entries, t.askedManifest(message.data, t.narrowing(old.rules))] }
      case 'exposure.request':
        return { ...old, entries: [...old.entries, t.askedExposure(message.data)] }
      case 'mcp-server.request':
        return { ...old, entries: [...old.entries, t.askedMcpServer(message.data)] }
      case 'mcp-tools.request':
        return { ...old, entries: [...old.entries, t.askedMcpTools(message.data)] }
      case 'mcp-call.request':
        return { ...old, entries: [...old.entries, t.askedMcpCall(message.data)] }
      case 'mcp-move.request':
        return { ...old, entries: [...old.entries, t.askedMcpMove(message.data)] }
      case 'host.request':
        return { ...old, entries: [...old.entries, t.askedHost(message.data)] }
      // A server has up to a minute to answer its handshake, and the turn says nothing else
      // while it waits, so the wait is named where the phase is.
      case 'mcp.starting':
        return { ...old, phase: 'starting-servers' }
      case 'mcp.started':
        return { ...old, phase: null, entries: [...old.entries, t.mcpStarted(message.data)] }
      // A run is not a turn, so it adds no turn marker and no reply to the conversation.
      case 'manifest.started':
        return { ...old, running: true, phase: null, checking: null, hook: null, tokens: 0,
          sandboxMode: message.data.sandbox ?? old.sandboxMode }
      case 'manifest.done':
        refresh()
        return { ...old, running: false, phase: null, checking: null, hook: null, outcome: 'complete',
          entries: [...old.entries, t.planReplied(message.data.reply, message.data.record)] }
      case 'manifest.error':
        refresh()
        return { ...old, running: false, phase: null, checking: null, hook: null, queuePaused: true,
          entries: [...t.interruptPending(old.entries), t.planEnded(message.data)] }
      case 'vouch.request':
        return { ...old, entries: [...old.entries, t.askedVouch(message.data)] }
      case 'ask.request':
        return { ...old, entries: [...old.entries, t.askedQuestions(message.data)] }
      case 'turn.done': {
        refresh()
        // A rise means compaction summarised part of this conversation away, and what it takes it
        // takes from the front — so a briefing put at the top of the session is the first thing
        // gone. Saying the bot is no longer grounded is what makes the next prompt carry it again.
        const compacted = message.data.archived > old.archived
        return {
          ...old,
          contextTokens: message.data.contextTokens,
          summary: { ...old.summary, id: message.data.id ?? old.summary.id },
          outcome: 'complete',
          running: message.data.consolidating === true,
          phase: null,
          // A consolidating turn stays running, so a check whose end was never heard would stay drawn.
          checking: null,
          hook: null,
          composing: null,
          entries: [...t.number(old.entries, old.awaitingOrdinal ?? '', message.data.prompt), t.replied(message.data.reply, message.data.turn)],
          awaitingOrdinal: null,
          archived: message.data.archived,
          rewind: message.data.rewind ?? [],
          bot: old.bot && compacted ? { ...old.bot, grounded: false } : old.bot,
        }
      }
      case 'turn.error': {
        refresh()
        const { kind, message: detail } = message.data
        return {
          ...old,
          contextTokens: message.data.contextTokens,
          summary: { ...old.summary, id: message.data.id ?? old.summary.id },
          running: false,
          phase: null,
          checking: null,
          hook: null,
          composing: null,
          entries: [...t.interruptPending(t.number(old.entries, old.awaitingOrdinal ?? '', message.data.prompt)), { ...t.errored(`${kind}: ${detail}`), category: kind === 'cancelled' ? 'cancelled' : message.data.category, attempts: message.data.attempts, status: message.data.status, cutOff: message.data.cutOff, turn: message.data.turn }],
          awaitingOrdinal: null,
          queuePaused: true,
          rewind: message.data.rewind ?? [],
        }
      }
      default:
        return old
    }
  })
}
