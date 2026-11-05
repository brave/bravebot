import { useCallback, useEffect, useId, useMemo, useRef, useState } from 'react'
import { flushSync } from 'react-dom'
import { providerLogo } from '../../shared/accounts'
import { arrange } from '../../shared/model-preferences'
import { EFFORT_LEVELS, type EffortLevel, type EffortReport, type ModelCatalogue, type ModelOption } from '../../shared/protocol'
import { setExperience, useExperience } from '../experience'
import { rememberCatalogue } from '../context-window'
import { useDelayedFlag, useFocusReturn } from '../hooks'
import { Alert, Button, ButtonMenu, Hr, Icon, Input, type IconName } from '../nala'
import { IconButton } from './IconButton'
import { ModelWarnings } from './ModelWarnings'
import { Skeleton } from './Skeleton'

/** Words a capability adds to what a search matches, so "vision" finds the models that see. */
const CAPABILITIES: Record<string, string> = {
  text: 'text generates text',
  vision: 'vision understands image input',
  'image-output': 'images out generates images',
  'audio-input': 'audio in accepts audio input',
  'audio-output': 'audio out generates audio',
  video: 'video accepts video input',
  files: 'files accepts file input',
  tools: 'tools supports tool calling',
  reasoning: 'reasoning supports reasoning parameters',
  'structured-output': 'structured supports structured outputs',
}

/** The rows of the Reasoning column: the service's own default, then the levels, cheapest first. */
const EFFORT_ROWS: readonly { level: EffortLevel | null; label: string }[] = [
  { level: null, label: 'Default' },
  ...EFFORT_LEVELS.map((level) => ({ level, label: ({ low: 'Low', medium: 'Medium', high: 'High', xhigh: 'Extra high', max: 'Ultra' } as const)[level] })),
]

/** How many rows PageDown / PageUp jumps. */
const PAGE_SIZE = 6

/** The rail entry that lists every service's models. */
const ALL = 'all'

/** Where the rail stood when a picker last closed, so the next one opens on it, in any conversation. */
const RAIL_KEY = 'bravebot.model-picker-rail'
const lastRail = (): string => { try { return localStorage.getItem(RAIL_KEY) || ALL } catch { return ALL } }
const keepRail = (key: string) => { try { localStorage.setItem(RAIL_KEY, key) } catch { /* the next open starts on All */ } }

/** Where the keyboard is: the service rail, the model list, or the Reasoning column. */
type Zone = 'rail' | 'models' | 'effort'

const searchable = (row: ModelOption) =>
  `${row.name} ${row.id} ${row.provider} ${(row.capabilities ?? []).map((key) => `${key} ${CAPABILITIES[key] ?? ''}`).join(' ')}`.toLowerCase()

/**
 * The model list for a conversation or a bot, and the effort level every turn asks for.
 *
 * Leo's ButtonMenu owns the popup: where it sits, the elevation, and dismissal when the pointer
 * goes outside. Inside it a rail of service marks sits on the left, the first listing every
 * service's models, then the models of the service chosen, then the Reasoning column. It opens on
 * the service it was left on, or on every service where that one is no longer listed.
 *
 * What a person hid or reordered in Settings › Providers is applied here (`arrange`), except that
 * the model in use is always listed. Typing searches every service at once.
 *
 * Choosing a model or a level keeps the menu open, so both can be chosen in one visit; Enter on a
 * model chooses it and closes. The level is the machine's, recorded where `/effort` records it, so
 * it applies to every conversation, and a checkout whose settings name one outranks it there.
 *
 * The keyboard stays in the search field. ArrowUp and ArrowDown move within the zone the
 * highlight is in, and with the field empty ArrowLeft and ArrowRight move between the rail, the
 * models and the Reasoning column. PageUp, PageDown, Home and End move through the models.
 */
export function ModelPicker({ model, disabled, onChoose, scope = 'conversation', session, compact = false }: {
  model: string | null
  scope?: 'conversation' | 'bot'
  session?: string
  /** Text and caret only, for the control that sits inside the composer. */
  compact?: boolean
  disabled: boolean
  onChoose: (model: string) => void
}): React.JSX.Element {
  const [open, setOpen] = useState(false)
  const preferences = useExperience()
  const [catalogue, setCatalogue] = useState<ModelCatalogue | null>(null)
  const [loading, setLoading] = useState(false)
  const showLoading = useDelayedFlag(loading)
  const [problem, setProblem] = useState<string | null>(null)
  const [effort, setEffort] = useState<EffortReport | null>(null)
  const [effortProblem, setEffortProblem] = useState<string | null>(null)
  const [query, setQuery] = useState('')
  const [rail, setRailState] = useState<string>(lastRail)
  const setRail = useCallback((key: string) => { setRailState(key); keepRail(key) }, [])
  const [zone, setZone] = useState<Zone>('models')
  const [active, setActive] = useState(0)
  const [activeEffort, setActiveEffort] = useState(0)
  const [revision, setRevision] = useState(0)
  /** The height the popup has between the trigger and the window's edge, on the side it opens. */
  const [room, setRoom] = useState<number | null>(null)
  const trigger = useRef<HTMLElement>(null)
  const search = useRef<HTMLElement>(null)
  const list = useRef<HTMLElement>(null)
  const byKeyboard = useRef(false)
  const { onClose, restoreFocus } = useFocusReturn(trigger)
  const id = useId()

  const close = () => {
    setOpen(false)
    trigger.current?.focus()
  }

  useEffect(() => {
    if (!open) return
    let gone = false
    setLoading(true)
    setProblem(null)
    void window.bravebot.request<ModelCatalogue>('models.list', { session }).then((answer) => {
      if (gone) return
      if (answer.error) setProblem(answer.error.message)
      else { setCatalogue(answer.ok ?? null); rememberCatalogue(session, answer.ok ?? null) }
    }).catch(() => {
      if (!gone) setProblem('Could not load models. Try again.')
    }).finally(() => { if (!gone) setLoading(false) })
    void window.bravebot.request<EffortReport>('effort.get', { session }).then((answer) => {
      if (gone) return
      if (answer.ok) { setEffort(answer.ok); setEffortProblem(null) } else setEffortProblem('Could not read the reasoning level. Try again.')
    }).catch(() => { if (!gone) setEffortProblem('Could not read the reasoning level. Try again.') })
    // Closing the menu abandons the load, so the next open waits its 150ms again.
    return () => { gone = true; setLoading(false) }
  }, [open, revision, session])

  /** Every model the picker may list, the one in use first among them where no service lists it. */
  const arranged = useMemo(() => {
    // A bridge from before rows named their service sends none, and its rows count as the default's.
    const rows = (catalogue?.models ?? []).map((row) => typeof row.providerKey === 'string' ? row : { ...row, providerKey: 'default' })
    if (model && !rows.some((row) => row.id === model)) {
      rows.unshift({ id: model, name: model, provider: 'Current selection', providerKey: 'current', readsEffort: true, premium: false, contextWindow: null })
    }
    return arrange(rows, preferences.providers, model)
  }, [catalogue, model, preferences.providers])

  const providerName = useCallback((key: string) =>
    arranged.models.find((row) => row.providerKey === key)?.provider ?? key, [arranged])

  const searching = query.trim() !== ''
  // A service hidden since it was chosen leaves the rail, and the list falls back to every service.
  const shownRail = rail !== ALL && arranged.providers.includes(rail) ? rail : ALL
  const options = useMemo(() => {
    if (searching) {
      const words = query.toLowerCase().trim().split(/\s+/)
      return arranged.models.filter((row) => { const text = searchable(row); return words.every((word) => text.includes(word)) })
    }
    return shownRail === ALL ? arranged.models : arranged.models.filter((row) => row.providerKey === shownRail)
  }, [arranged, query, searching, shownRail])
  /** Rows carry their service's mark where more than one service is listed. */
  const marked = searching || shownRail === ALL
  const railKeys = useMemo(() => [ALL, ...arranged.providers], [arranged.providers])

  useEffect(() => {
    if (!open) return
    search.current?.focus()
  }, [open])
  // Leo opens the popup on whichever side of the trigger has more room and scrolls the whole of it
  // when it is taller than that. Sizing it to the room instead keeps the search field and the
  // footnote in view and scrolls only the lists.
  useEffect(() => {
    if (!open) return
    const measure = () => {
      const box = trigger.current?.getBoundingClientRect()
      if (box) setRoom(Math.max(box.top, window.innerHeight - box.bottom) - 24)
    }
    measure()
    window.addEventListener('resize', measure)
    return () => window.removeEventListener('resize', measure)
  }, [open])

  // The field Playwright and the accessibility tree fill is the inner input.
  // A role on the Leo host makes a second combobox that cannot be typed into.
  const activeId = zone === 'rail' ? `${id}-rail-${railKeys.indexOf(shownRail)}`
    : zone === 'effort' ? `${id}-effort-${activeEffort}`
      : options[active] ? `${id}-option-${active}` : null
  useEffect(() => {
    const inner = search.current?.shadowRoot?.querySelector('input')
    if (!inner) return
    inner.setAttribute('role', 'combobox')
    inner.setAttribute('aria-label', 'Search models')
    inner.setAttribute('aria-autocomplete', 'list')
    inner.setAttribute('aria-expanded', String(open))
    inner.setAttribute('aria-controls', `${id}-list`)
    if (open && activeId) inner.setAttribute('aria-activedescendant', activeId)
    else inner.removeAttribute('aria-activedescendant')
  }, [open, id, activeId])

  useEffect(() => { if (disabled) setOpen(false) }, [disabled])
  // A new list starts on the model in use where it is in it, and on the first row otherwise.
  useEffect(() => {
    const current = options.findIndex((row) => row.id === model)
    setActive(current >= 0 && !searching ? current : 0)
    // Only when what is listed changes, not when a choice moves the model in use.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, catalogue, shownRail])
  useEffect(() => {
    if (list.current) list.current.scrollTop = 0
  }, [query, shownRail])
  useEffect(() => {
    if (!byKeyboard.current) return
    byKeyboard.current = false
    const frame = list.current
    const row = frame?.querySelector<HTMLElement>(`[data-index="${active}"]`)
    if (!frame || !row) return
    const frameBox = frame.getBoundingClientRect()
    const rowBox = row.getBoundingClientRect()
    if (rowBox.top < frameBox.top) frame.scrollTop -= frameBox.top - rowBox.top
    else if (rowBox.bottom > frameBox.bottom) frame.scrollTop += rowBox.bottom - frameBox.bottom
  }, [active])

  const heading = scope === 'bot' ? 'Bot model' : 'Conversation model'
  const selected = catalogue?.models.find((row) => row.id === model)
  const label = selected?.name ?? model ?? 'Configured default'
  const compactLabel = label.split('/').pop() || label
  const inUse = catalogue?.models.find((row) => row.id === (model ?? catalogue.defaultModel))
  const takesNoLevel = inUse?.readsEffort === false
  const effortLocked = !effort || effort.outranked || takesNoLevel
  const listTitle = searching ? 'Results' : shownRail === ALL ? 'All models' : providerName(shownRail)

  // Leo's React wrapper assigns `aria-label` as a property, and the button host
  // only exposes `ariaLabel`, so the attribute the accessibility tree and the
  // drive test read would otherwise stay empty.
  useEffect(() => {
    const button = trigger.current
    if (!button) return
    button.setAttribute('aria-label', `Choose model: ${label}`)
    button.setAttribute('aria-haspopup', 'dialog')
    button.setAttribute('aria-expanded', String(open))
    if (open) button.setAttribute('aria-controls', id)
    else button.removeAttribute('aria-controls')
  }, [label, open, id])

  const choose = (row: ModelOption, then: 'stay' | 'close') => {
    setExperience('recentModels', [row.id, ...preferences.recentModels.filter((id) => id !== row.id)].slice(0, 8))
    onChoose(row.id)
    if (then === 'close') close()
  }

  const chooseEffort = (level: EffortLevel | null) => {
    if (effortLocked) return
    const was = effort
    setEffort((report) => report && { ...report, recorded: level, inForce: level })
    void window.bravebot.request<{ recorded: EffortLevel | null }>('effort.set', { level }).then((answer) => {
      if (answer.error) { setEffort(was); setEffortProblem(answer.error.message) } else setEffortProblem(null)
    }).catch(() => { setEffort(was); setEffortProblem('Could not save the reasoning level. Try again.') })
  }

  const chooseRail = (key: string) => {
    setRail(key)
    setQuery('')
    search.current?.focus()
  }

  const handleKey = useCallback((key: string): boolean => {
    const move = (count: number, length: number, set: (update: (index: number) => number) => void) => {
      if (!length) return
      set((index) => Math.min(length - 1, Math.max(0, index + count)))
    }
    if (key === 'ArrowDown' || key === 'ArrowUp') {
      const dir = key === 'ArrowDown' ? 1 : -1
      byKeyboard.current = true
      if (zone === 'rail') {
        const next = railKeys[Math.min(railKeys.length - 1, Math.max(0, railKeys.indexOf(shownRail) + dir))]
        if (next) setRail(next)
      } else if (zone === 'effort') move(dir, EFFORT_ROWS.length, setActiveEffort)
      else move(dir, options.length, setActive)
      return true
    }
    if (zone === 'models' && (key === 'PageDown' || key === 'PageUp')) {
      byKeyboard.current = true
      move((key === 'PageDown' ? 1 : -1) * PAGE_SIZE, options.length, setActive)
      return true
    }
    if (zone === 'models' && (key === 'Home' || key === 'End')) {
      // With text in the field the caret takes them, as it takes ArrowLeft and ArrowRight.
      if (searching) return false
      byKeyboard.current = true
      setActive(key === 'Home' ? 0 : Math.max(0, options.length - 1))
      return true
    }
    // Only claimed with the field empty: with text in it the caret moves instead.
    if ((key === 'ArrowLeft' || key === 'ArrowRight') && !searching) {
      const order: Zone[] = ['rail', 'models', 'effort']
      const next = order[order.indexOf(zone) + (key === 'ArrowRight' ? 1 : -1)]
      if (!next) return true
      if (next === 'effort') setActiveEffort(Math.max(0, EFFORT_ROWS.findIndex((row) => row.level === (effort?.recorded ?? null))))
      setZone(next)
      return true
    }
    return false
  }, [zone, railKeys, shownRail, options.length, searching, effort])

  // Leo hands a changed `onKeyDown` to its field a tick after a render, and the field's events are
  // not React's, so what one key changes is drawn after the next key has arrived. The field is given
  // one function for good, which reads the handler of the latest render and draws what the key
  // changed before returning, so each key is read against the state the one before it left.
  const searchKey = useRef<(innerEvent: Event) => void>(() => {})
  searchKey.current = (innerEvent: Event) => {
    const key = (innerEvent as unknown as KeyboardEvent).key
    if (handleKey(key)) {
      innerEvent.preventDefault()
      innerEvent.stopPropagation()
    } else if (key === 'Enter') {
      innerEvent.preventDefault()
      innerEvent.stopPropagation()
      if (zone === 'effort') { const row = EFFORT_ROWS[activeEffort]; if (row) chooseEffort(row.level) }
      else if (zone === 'rail') setZone('models')
      else { const row = options[Math.min(active, options.length - 1)]; if (row) choose(row, 'close') }
    } else if (key === 'Escape') {
      innerEvent.preventDefault()
      innerEvent.stopPropagation()
      close()
    }
  }
  const onSearchKey = useCallback(({ innerEvent }: { innerEvent: Event }) => flushSync(() => searchKey.current(innerEvent)), [])

  return <div className="model-picker">
    <ButtonMenu
      className="model-menu"
      isOpen={open && !disabled}
      placement="top-end"
      positionStrategy="fixed"
      onClose={onClose}
      onChange={({ isOpen: next }) => {
        if (next) {
          if (disabled) return
          setQuery('')
          setRailState(lastRail())
          setZone('models')
          setOpen(true)
          return
        }
        setOpen(false)
        // Outside click is a blur: leave focus where the pointer landed. Escape
        // and a choice return to the button that opened the menu.
        restoreFocus()
      }}
    >
      <Button ref={trigger} slot="anchor-content" kind={scope === 'bot' && !compact ? 'plain' : 'plain-faint'} size={compact ? 'tiny' : 'medium'} className="model-trigger" isDisabled={disabled}
        data-tooltip={`Choose model · ${model ?? label}`} aria-label={`Choose model: ${label}`}
        aria-haspopup="dialog" aria-expanded={open} aria-controls={open ? id : undefined}
        data-test="model-trigger">
        {compact ? null : <Icon name="layers" slot="icon-before" />}
        <span className="model-current" aria-hidden="true">{compactLabel}</span>
        <Icon name={open ? 'carat-up' : 'carat-down'} slot="icon-after" />
      </Button>
      <div className="model-popover" id={id} role="group" aria-label={heading} data-test="model-popover"
        style={room ? { '--model-room': `${room}px` } as React.CSSProperties : undefined}>
        <div className="model-search-row">
          <Input ref={search} className="model-search" type="search" size="small" placeholder="Search models…" value={query}
            data-test="model-search"
            onInput={({ value }) => flushSync(() => { setQuery(value); setZone('models'); setActive(0) })}
            onChange={({ value }) => setQuery(value)}
            onKeyDown={onSearchKey}>
            <Icon name="search" slot="left-icon" />
          </Input>
          <Button size="small" kind="plain-faint" className="model-refresh" isDisabled={loading} onClick={() => setRevision((n) => n + 1)} data-test="model-refresh">
            <Icon name="refresh" slot="icon-before" />Refresh
          </Button>
        </div>
        <Hr />
        <div className="model-notices">
          {problem && <Alert type="error" size="small" className="model-status" role="alert">{problem}</Alert>}
          {catalogue && <ModelWarnings warnings={catalogue.warnings} commands={catalogue.warningCommands} session={session} onSignedIn={() => setRevision((n) => n + 1)} />}
        </div>
        <div className="model-columns">
          <div className="model-rail" role="toolbar" aria-orientation="vertical" aria-label="Model services" data-test="model-rail">
            {railKeys.map((key, index) => {
              const name = key === ALL ? 'All models' : providerName(key)
              const icon = (key === ALL ? 'content-compact' : providerLogo(key)) as IconName
              return <IconButton key={key} icon={icon} label={name} placement="right" size="small"
                pressed={!searching && key === shownRail}
                className={`model-rail-item${zone === 'rail' && key === shownRail ? ' active' : ''}`}
                id={`${id}-rail-${index}`} data-test={`model-rail-${key}`}
                onClick={() => chooseRail(key)} />
            })}
          </div>
          <div className="model-column model-list-column">
            <div className="model-column-title" id={`${id}-list-title`} data-test="model-list-title">{listTitle}</div>
            <leo-menu-section id={`${id}-list`} role="listbox" aria-labelledby={`${id}-list-title`} ref={list}>
              {options.map((row, index) => (
                <leo-option key={`${row.providerKey}:${row.id}`} id={`${id}-option-${index}`} value={row.id}
                  role="option" aria-selected={row.id === model ? 'true' : 'false'} data-is-interactive="true"
                  data-index={index} data-current={row.id === model ? 'true' : undefined} data-provider={row.providerKey}
                  className={zone === 'models' && index === active ? 'active' : undefined}
                  onMouseDown={(event) => event.preventDefault()}
                  onMouseEnter={() => { setZone('models'); setActive(index) }}
                  onClick={() => choose(row, 'stay')}>
                  {marked && <Icon className="model-provider-mark" name={providerLogo(row.providerKey) as IconName} aria-hidden="true" data-test="model-provider-mark" />}
                  <span className="model-description">
                    <span className="model-name" data-tooltip={row.name}>{row.name}</span>
                    <span className="model-detail">{[
                      row.id === catalogue?.defaultModel ? 'Default' : null,
                      row.premium ? 'Premium' : null,
                      row.contextWindow ? `${row.contextWindow.toLocaleString()} context tokens` : null,
                      preferences.recentModels.includes(row.id) ? 'Recent' : null,
                    ].filter(Boolean).join(' · ')}</span>
                  </span>
                  <span className="model-check" aria-hidden="true">{row.id === model ? <Icon name="check-normal" /> : null}</span>
                </leo-option>
              ))}
            </leo-menu-section>
            {/* Rows the shape of the ones to come, until the first listing arrives. Mounted when the
                load starts and hidden for its first 150ms, so a quick answer draws no flash. */}
            {loading && !catalogue && <Skeleton label="Loading available models" count={4} mark delayed={!showLoading}
              className="model-skeleton" data-test="model-skeleton" />}
            {!loading && options.length === 0 && <p className="model-status" data-test="model-empty">
              {searching ? 'No models match your search.' : catalogue?.models.length ? 'Every model is hidden. Show some in Settings, under Providers.' : 'No models available. Check your backend settings.'}
            </p>}
          </div>
          <div className="model-column model-effort-column" data-test="model-effort">
            <div className="model-column-title" id={`${id}-effort-title`}>Reasoning</div>
            <div className="model-effort-rows" role="radiogroup" aria-labelledby={`${id}-effort-title`} aria-disabled={effortLocked ? 'true' : undefined}>
              {EFFORT_ROWS.map((row, index) => {
                const checked = !!effort && (effort.outranked ? effort.inForce : effort.recorded) === row.level
                return <leo-option key={row.label} id={`${id}-effort-${index}`} role="radio" aria-checked={checked ? 'true' : 'false'}
                  aria-disabled={effortLocked ? 'true' : undefined} data-is-interactive="true" data-level={row.level ?? 'default'}
                  data-current={checked ? 'true' : undefined} data-test={`model-effort-${row.level ?? 'default'}`}
                  className={zone === 'effort' && index === activeEffort ? 'active' : undefined}
                  onMouseDown={(event) => event.preventDefault()}
                  onMouseEnter={() => { setZone('effort'); setActiveEffort(index) }}
                  onClick={() => chooseEffort(row.level)}>
                  <span className="model-name">{row.label}</span>
                  <span className="model-check" aria-hidden="true">{checked ? <Icon name="check-normal" /> : null}</span>
                </leo-option>
              })}
            </div>
            {effort?.outranked
              ? <p className="model-effort-note" data-test="model-effort-note">This folder’s settings set the level.</p>
              : takesNoLevel && <p className="model-effort-note" data-test="model-effort-note">This model takes no reasoning level.</p>}
            {effortProblem && <p className="model-effort-note" role="alert">{effortProblem}</p>}
          </div>
        </div>
        <Hr />
        <p className="model-footnote">{scope === 'bot' ? 'Saved with this bot. Applies to its next message.' : 'Applies to the next message in this conversation.'} The reasoning level applies to every conversation. Brave Bot uses text and tools; pricing is not supplied by this catalogue.</p>
      </div>
    </ButtonMenu>
  </div>
}
