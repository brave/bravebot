import { memo, useEffect, useMemo, useRef, useState } from 'react'
import { attachCommand, sessionsOf, type BackgroundGroup, type BackgroundList, type BackgroundRow } from '../background-list'
import { middleTruncate } from '../truncate'
import { CopyButton } from './CopyButton'
import { ago } from './Sessions'

/** How often the roster is read again while the section can be seen (BG-17), as the terminal's `REFRESH`. */
const REFRESH_MS = 1000

/** How much of the working directory the detail shows before cutting it in the middle. */
const DIRECTORY_WIDTH = 36

/**
 * The background sessions of every project (BG-17), above the chats: a heading for each state in
 * BG-16 order with its count, a row for each session, and the directory and last prompt of the
 * selected one below them.
 *
 * Read only. A row selects and does nothing else: attaching, replying, stopping and starting a
 * session are done from a terminal (BG-9, BG-2), so the one action is copying `bravebot attach`.
 * Every text drawn here is a field the bridge sent, with its control characters already pictured.
 *
 * The section is in the tree while the roster is empty, hidden, so it can tell whether the column
 * is shown and keep polling for the first session to appear.
 */
export function BackgroundSessions(): React.JSX.Element {
  const [groups, setGroups] = useState<BackgroundGroup[]>([])
  const [failed, setFailed] = useState(false)
  const [selected, setSelected] = useState<string | null>(null)
  const section = useRef<HTMLElement>(null)

  useEffect(() => {
    let live = true
    let timer: ReturnType<typeof setTimeout> | undefined
    let reading = false
    // Shown means the window is not hidden and the list this sits in is drawn: the Chats tab is
    // chosen and the column is not folded away.
    const shown = (): boolean => !document.hidden && (section.current?.parentElement?.checkVisibility() ?? false)
    const read = async (): Promise<void> => {
      if (reading || !shown()) return
      reading = true
      try {
        const answer = await window.bravebot.request<BackgroundList>('background.list')
        if (!live) return
        if (answer.error) setFailed(true)
        else {
          setFailed(false)
          setGroups(answer.ok?.groups ?? [])
        }
      } catch {
        if (live) setFailed(true)
      } finally {
        reading = false
      }
    }
    const tick = (): void => {
      void read().finally(() => { if (live) timer = setTimeout(tick, REFRESH_MS) })
    }
    const again = (): void => { if (!document.hidden) void read() }
    tick()
    document.addEventListener('visibilitychange', again)
    return () => {
      live = false
      clearTimeout(timer)
      document.removeEventListener('visibilitychange', again)
    }
  }, [])

  const rows = useMemo(() => sessionsOf(groups), [groups])
  // Selection is by id, so a session whose state changed is still the one selected under its new
  // heading, and one that left the roster is no longer selected.
  const chosen = rows.find((row) => row.id === selected)

  return (
    <section ref={section} className="background-sessions" data-test="background-sessions" hidden={rows.length === 0}
      aria-labelledby="background-sessions-title">
      <h2 className="background-title" id="background-sessions-title">Background sessions</h2>
      {groups.map((group) => (
        <div key={group.state} className="background-group" role="group" aria-label={group.heading} data-state={group.state}>
          <div className="background-group-head">
            <span className="background-group-name">{group.heading}</span>
            <span className="count num">{group.count}</span>
          </div>
          {group.sessions.map((row) => (
            <BackgroundSession key={row.id} row={row} selected={row.id === selected} onSelect={setSelected} />
          ))}
        </div>
      ))}
      {failed && <p className="background-note" role="status">Could not read the background sessions. Trying again.</p>}
      {chosen && (
        <div className="background-detail" data-test="background-detail" aria-label={`Session ${chosen.short}`} role="region">
          <dl>
            <dt>Directory</dt>
            <dd><code data-tooltip={chosen.directory}>{middleTruncate(chosen.directory, DIRECTORY_WIDTH)}</code></dd>
            <dt>Last prompt</dt>
            <dd className="background-prompt">{chosen.prompt || 'None yet'}</dd>
          </dl>
          <div className="background-attach">
            <code className="background-command">{attachCommand(chosen)}</code>
            <CopyButton text={() => attachCommand(chosen)} label="Copy attach command" data-test="copy-attach" />
          </div>
        </div>
      )}
    </section>
  )
}

/** One session: its short id and how long since it was written, its name, and its state. */
const BackgroundSession = memo(function BackgroundSession({ row, selected, onSelect }: {
  row: BackgroundRow
  selected: boolean
  onSelect: (id: string) => void
}): React.JSX.Element {
  return (
    <button type="button" className={`background-row${selected ? ' current' : ''}`} aria-pressed={selected}
      data-session={row.id} onClick={() => onSelect(row.id)}>
      <span className="background-meta">
        <code className="background-short">{row.short}</code>
        <time className="background-time num" dateTime={new Date(row.updated * 1000).toISOString()}>{ago(row.updated)}</time>
      </span>
      <span className="background-name" data-tooltip={row.name}>{row.name}</span>
      <span className="background-state">{row.words}</span>
    </button>
  )
})
