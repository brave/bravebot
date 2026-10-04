import { useEffect, useMemo, useRef, useState } from 'react'
import { botProjects, type Bot } from '../../shared/bots'
import type { BotConversation } from '../../shared/bot-history'
import type { SessionSummary } from '../../shared/protocol'
import { projectLabel } from '../../shared/recents'
import { shortAgo } from '../time'
import { Icon, Input } from '../nala'
import { BotAvatar } from './BotAvatar'
import { Composer, type ComposerFooterProps, type ProjectChoice } from './Composer'

const noop = () => {}

/**
 * A bot's own page, before any one conversation is open: its recent conversations, and a composer
 * that starts a new one.
 *
 * The project is chosen in the composer's footer and defaults to none, which runs the conversation
 * in the bot's home folder. Only folders this bot has worked in, or one picked here, are offered,
 * because those are the only ones the main process lets a bot work in.
 */
export function BotView({ notices, bot, history, filtering, backendReady, onOpen, onStart, onModel, onSetup, onCheckBackend, onDiagnostics }: {
  /** Errors and confirmations, shown above the composer. */
  notices: React.ReactNode
  bot: Bot
  history: BotConversation[]
  /** Whether the header's search is open, which filters the list. */
  filtering: boolean
  backendReady: boolean | null
  onOpen: (summary: SessionSummary) => void
  onStart: (prompt: string, directory: string | null) => void
  onModel: (model: string) => void
  onSetup: () => void
  onCheckBackend: () => void
  onDiagnostics: () => void
}): React.JSX.Element {
  const input = useRef<HTMLElement>(null)
  const [draft, setDraft] = useState('')
  const [project, setProject] = useState<string | null>(null)
  const [picked, setPicked] = useState<string[]>([])
  const [query, setQuery] = useState('')
  useEffect(() => { setDraft(''); setProject(null); setPicked([]); setQuery('') }, [bot.slug])
  useEffect(() => { if (!filtering) setQuery('') }, [filtering])

  const shown = useMemo(() => {
    const terms = query.toLowerCase().split(/\s+/).filter(Boolean)
    return history.filter((row) => terms.every((term) =>
      `${row.session?.title ?? row.id} ${row.directory === bot.home ? 'No project' : projectLabel(row.directory)}`.toLowerCase().includes(term)))
  }, [history, query, bot.home])

  const start = () => {
    const prompt = draft.trim()
    if (!prompt || backendReady === false) return
    // The draft stays: a conversation that opens replaces this page, and one that fails to open
    // leaves the message here to send again.
    onStart(prompt, project)
  }
  const footer: ComposerFooterProps = {
    directory: project,
    branch: null,
    choices: { folders: [...new Set([...picked, ...botProjects(bot)])], noProject: true },
    onChoose: (choice: ProjectChoice) => {
      if (choice.kind === 'none') setProject(null)
      else if (choice.kind === 'folder') setProject(choice.directory)
      else void window.bravebot.chooseDirectory().then((folder) => {
        if (!folder) return
        setPicked((old) => [folder, ...old.filter((each) => each !== folder)])
        setProject(folder)
      })
    },
  }

  const composer = (
    <Composer
      input={input}
      session=""
      model={bot.model}
      running={false}
      askingTrust={false}
      compacting={false}
      pending={false}
      scope="bot"
      draft={draft}
      onDraft={setDraft}
      onSend={start}
      onQueue={start}
      onCancel={noop}
      onModel={onModel}
      attachments={[]}
      onAttach={noop}
      onRemoveAttachment={noop}
      onPreview={noop}
      queued={[]}
      queuePaused={false}
      onResumeQueued={noop}
      onRemoveQueued={noop}
      backendReady={backendReady}
      onSetup={onSetup}
      onCheckBackend={onCheckBackend}
      onDiagnostics={onDiagnostics}
      canAttach={false}
      footer={footer}
    />
  )

  // A bot nobody has talked to yet: its face, that it is ready, and the box to start with.
  if (history.length === 0) {
    return (
      <div className="entries bot-view-body bot-first" data-test="bot-page">
        <div className="bot-first-column">
          <div className="bot-first-hero">
            <BotAvatar seed={bot.avatar} size={160} doing="open" />
            <h1>{bot.name} is ready</h1>
            <p>Get started with your bot. Add it to a project or just chat with it.</p>
          </div>
          <div className="composer-dock"><div className="dock-float">{notices}</div>{composer}</div>
        </div>
      </div>
    )
  }

  return (
    <>
      <div className="entries bot-view-body" data-test="bot-page">
        <div className="bot-view-column">
          {filtering && (
            <Input autofocus type="search" size="small" className="bot-history-search" aria-label="Search conversations"
              placeholder="Search conversations" value={query} onInput={({ value }) => setQuery(value)}>
              <Icon name="search" slot="left-icon" />
            </Input>
          )}
          <h2 className="bot-view-title">Recent conversations</h2>
          <div className="bot-conversations" aria-label={`${bot.name}’s conversations`} data-test="bot-conversations">
            {shown.map(({ id, directory, session, archived }) => {
              const where = directory === bot.home ? 'No project' : projectLabel(directory)
              // A conversation the bot recorded that the chat list no longer holds: said, not hidden.
              if (!session) return (
                <div key={`${directory}/${id}`} className="bot-history-row unavailable">
                  <span className="session-meta"><span className="session-project">{where}</span></span>
                  <span className="session-title"><span className="session-name">Unavailable conversation</span></span>
                  <span className="session-where">Not in the chat list now · <code>{id}</code></span>
                </div>
              )
              return (
                <button type="button" key={`${directory}/${id}`} className="bot-history-row" onClick={() => onOpen(session)}>
                  <span className="session-meta">
                    <span className="session-project">{where}</span>
                    {archived && <span className="bot-history-archived">Archived</span>}
                    <time className="session-time num" dateTime={new Date(session.updated * 1000).toISOString()}>{shortAgo(session.updated)}</time>
                  </span>
                  <span className="session-title"><span className="session-name">{session.title}</span></span>
                  {session.branch && <span className="session-where"><span className="branch">{session.branch}</span></span>}
                </button>
              )
            })}
            {shown.length === 0 && (
              <p className="bot-history-empty">
                <Icon name="message-bubble" />
                {query.trim() ? `No conversation matches “${query}”.` : 'No conversations yet. Send a message below to start one.'}
              </p>
            )}
          </div>
        </div>
      </div>
      <div className="composer-dock"><div className="dock-float">{notices}</div>{composer}</div>
    </>
  )
}
