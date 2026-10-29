import { useEffect, useMemo, useState, useSyncExternalStore } from 'react'
import { Button, Checkbox, Icon } from '../src/renderer/nala'
import { onScreen, play, playground, playing, stop, writeConfigured, type Speed } from './bridge'
import { SCENARIOS } from './scenarios'

const SPEEDS: Speed[] = ['instant', 'fast', 'normal', 'slow']
const THEMES = ['system', 'light', 'dark'] as const

/** Re-render when the bridge says its settings or its running turn changed. */
function useBridge(): number {
  return useSyncExternalStore(
    (listen) => { playground.listeners.add(listen); return () => playground.listeners.delete(listen) },
    () => version,
  )
}
let version = 0
playground.listeners.add(() => { version++ })

function Chips<T extends string>({ label, value, options, onChange }: { label: string; value: T; options: readonly T[]; onChange: (next: T) => void }): React.JSX.Element {
  return (
    <div className="pg-row">
      <span className="pg-label">{label}</span>
      <div className="pg-chips" role="group" aria-label={label}>
        {options.map((option) => (
          <button key={option} type="button" className="pg-chip" aria-pressed={option === value} onClick={() => onChange(option)}>{option}</button>
        ))}
      </div>
    </div>
  )
}

/**
 * The controls beside the app: which scenario to play, how fast, and whether the harness answers
 * the cards. It is not part of the product and nothing here is reachable from a build of it.
 */
export function Panel(): React.JSX.Element {
  useBridge()
  const [open, setOpen] = useState(() => localStorage.getItem('playground:open') === 'true')
  const [theme, setTheme] = useState<(typeof THEMES)[number]>(() => (localStorage.getItem('playground:theme') as (typeof THEMES)[number] | null) ?? 'system')
  const [note, setNote] = useState<string | null>(null)
  const [query, setQuery] = useState('')
  const groups = useMemo(() => {
    const found = new Map<string, typeof SCENARIOS>()
    for (const scenario of SCENARIOS) {
      if (query && !`${scenario.title} ${scenario.detail} ${scenario.group}`.toLowerCase().includes(query.toLowerCase())) continue
      found.set(scenario.group, [...(found.get(scenario.group) ?? []), scenario])
    }
    return [...found]
  }, [query])
  useEffect(() => { localStorage.setItem('playground:open', String(open)) }, [open])

  const handle = onScreen()
  const busy = handle ? playing(handle) : false
  const start = (id: string): void => {
    const scenario = SCENARIOS.find((each) => each.id === id)
    const target = onScreen()
    if (!scenario) return
    if (!target) { setNote('Open a conversation first, or press New chat.'); return }
    if (!play(target, scenario.title, scenario.run)) { setNote('A turn is already running here. Stop it first.'); return }
    setNote(null)
  }

  if (!open) {
    return (
      <button type="button" className="pg-pill" onClick={() => setOpen(true)} aria-label="Open the playground">
        <Icon name="play-circle" />Playground
      </button>
    )
  }
  return (
    <aside className="pg-panel" aria-label="Playground">
      <header className="pg-head">
        <strong>Playground</strong>
        <span className="pg-sub">The real app, answered from fixtures</span>
        <button type="button" className="pg-close" onClick={() => setOpen(false)} aria-label="Collapse the playground"><Icon name="minus" /></button>
      </header>
      <div className="pg-body">
        <Chips label="Theme" value={theme} options={THEMES} onChange={(next) => { setTheme(next); window.bravebot.writeTheme(next) }} />
        <Chips label="Speed" value={playground.speed} options={SPEEDS} onChange={(next) => { playground.speed = next; playground.changed() }} />
        <div className="pg-row">
          <Checkbox checked={playground.auto} size="small" onChange={({ checked }) => { playground.auto = checked; playground.changed() }}>Answer questions for me</Checkbox>
        </div>
        <div className="pg-row">
          <Checkbox checked={!playground.configured} size="small" onChange={({ checked }) => writeConfigured(!checked)}>Backend not set up (reload to see it)</Checkbox>
        </div>
        <p className="pg-hint">Type <code>play:tour</code> in the composer to run a scenario by id. Anything else gets a short reply.</p>
        {busy && handle && (
          <div className="pg-running" role="status">
            <span>Playing: {playground.running}</span>
            <Button size="small" kind="outline" onClick={() => stop(handle)}>Stop</Button>
          </div>
        )}
        {note && <p className="pg-note" role="status">{note}</p>}
        <input className="pg-search" type="search" placeholder="Find a scenario" aria-label="Find a scenario" value={query} onChange={(event) => setQuery(event.target.value)} />
        {groups.map(([group, items]) => (
          <details key={group} className="pg-group" open={query !== '' || group === 'Turns'}>
            <summary>{group}<span className="pg-count">{items.length}</span></summary>
            <ul>
              {items.map((scenario) => (
                <li key={scenario.id}>
                  <button type="button" className="pg-item" onClick={() => start(scenario.id)} disabled={busy}>
                    <span className="pg-title">{scenario.title}</span>
                    <span className="pg-detail">{scenario.detail}</span>
                  </button>
                </li>
              ))}
            </ul>
          </details>
        ))}
        {groups.length === 0 && <p className="pg-hint">Nothing matches.</p>}
      </div>
    </aside>
  )
}
