import { Fragment, useCallback, useEffect, useId, useMemo, useRef, useState } from 'react'
import type { ModelCatalogue, ModelOption } from '../../shared/protocol'
import { setExperience, useExperience } from '../experience'
import { rememberCatalogue } from '../context-window'
import { useDelayedFlag, useFocusReturn } from '../hooks'
import { Alert, Button, ButtonMenu, Hr, Icon, Input, Label, ProgressRing, type IconName } from '../nala'
import { ModelWarnings } from './ModelWarnings'

/** The two capabilities this app uses; the rest are in the catalogue but not shown. */
const CAPABILITY_ICONS: Record<string, IconName> = { text: 'message-bubble-text', tools: 'wrench' }

const CAPABILITIES: Record<string, [string, string]> = {
  text: ['Text', 'Generates text'],
  vision: ['Vision', 'Understands image input'],
  'image-output': ['Images out', 'Generates images'],
  'audio-input': ['Audio in', 'Accepts audio input'],
  'audio-output': ['Audio out', 'Generates audio'],
  video: ['Video', 'Accepts video input'],
  files: ['Files', 'Accepts file input'],
  tools: ['Tools', 'Supports tool calling'],
  reasoning: ['Reasoning', 'Supports reasoning parameters'],
  'structured-output': ['Structured', 'Supports structured outputs'],
}

/** How many rows PageDown / PageUp jumps. */
const PAGE_SIZE = 6

/**
 * The model list for a conversation or a bot.
 *
 * Leo's ButtonMenu owns the popup: where it sits, the elevation, and dismissal
 * when the pointer goes outside. Rows are `leo-option`s, the search field is a
 * Leo input, and the chips are Leo labels.
 *
 * Hover used to close this menu. Entering a row called `scrollIntoView`, which
 * scrolls every ancestor, and a window scroll listener read that as the page
 * moving and shut the popup. The keyboard highlight only moves the list's own
 * scroll offset, so a pointer moving across the rows leaves the menu up.
 *
 * Provider sections are collapsible. ArrowLeft on any row in a section, or on
 * the section heading itself, collapses it and moves the highlight to the first
 * visible row at or after the collapsed block. ArrowRight on a collapsed heading
 * re-expands it. PageDown / PageUp jump PAGE_SIZE rows at a time through
 * visible (non-collapsed) rows. Home / End jump to the first / last visible row.
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
  const [query, setQuery] = useState('')
  const [active, setActive] = useState(0)
  const [revision, setRevision] = useState(0)
  /** Providers whose rows are hidden. Cleared when the query changes (search shows all). */
  const [collapsedProviders, setCollapsedProviders] = useState<ReadonlySet<string>>(() => new Set())
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
    // Closing the menu abandons the load, so the next open waits its 150ms again.
    return () => { gone = true; setLoading(false) }
  }, [open, revision, session])

  const options = useMemo(() => {
    const rows = [...(catalogue?.models ?? [])]
    if (model && !rows.some((row) => row.id === model)) {
      rows.unshift({ id: model, name: model, provider: 'Current selection', premium: false, contextWindow: null })
    }
    const words = query.toLowerCase().trim().split(/\s+/)
    const sorted = rows.filter((row) => {
      const capabilities = (row.capabilities ?? []).map((key) =>
        `${key} ${CAPABILITIES[key]?.join(' ') ?? ''}`).join(' ')
      const searchable = `${row.name} ${row.id} ${row.provider} ${capabilities}`.toLowerCase()
      return words.every((word) => searchable.includes(word))
    }).sort((a, b) => {
      const rank = (row: ModelOption) => row.id === model ? -2 : preferences.recentModels.includes(row.id) ? preferences.recentModels.indexOf(row.id) : 100
      return rank(a) - rank(b)
    })
    // Rows sit under their provider. Providers keep the order of their best-ranked row, so the
    // current and recent models stay at the top; the keyboard walks this same flat order.
    const providers: string[] = []
    for (const row of sorted) if (!providers.includes(row.provider)) providers.push(row.provider)
    return providers.flatMap((provider) => sorted.filter((row) => row.provider === provider))
  }, [catalogue, model, query, preferences.recentModels])

  /** Indices of rows that are visible (their provider is not collapsed). */
  const visibleIndices = useMemo(
    () => options.map((row, i) => ({ row, i })).filter(({ row }) => !collapsedProviders.has(row.provider)).map(({ i }) => i),
    [options, collapsedProviders],
  )

  /** Clamp active to the nearest visible index at or after target. Falls back to last visible. */
  const nearestVisible = useCallback((target: number, indices: readonly number[]): number => {
    if (!indices.length) return 0
    return indices.find((i) => i >= target) ?? indices[indices.length - 1]!
  }, [])

  useEffect(() => {
    if (!open) return
    search.current?.focus()
  }, [open])

  // The field Playwright and the accessibility tree fill is the inner input.
  // A role on the Leo host makes a second combobox that cannot be typed into.
  useEffect(() => {
    const inner = search.current?.shadowRoot?.querySelector('input')
    if (!inner) return
    inner.setAttribute('role', 'combobox')
    inner.setAttribute('aria-label', 'Search models')
    inner.setAttribute('aria-autocomplete', 'list')
    inner.setAttribute('aria-expanded', String(open))
    inner.setAttribute('aria-controls', `${id}-list`)
    if (open && options[active]) inner.setAttribute('aria-activedescendant', `${id}-option-${active}`)
    else inner.removeAttribute('aria-activedescendant')
  }, [open, id, active, options])

  useEffect(() => { if (disabled) setOpen(false) }, [disabled])
  useEffect(() => {
    // When query or catalogue changes, reset collapsed state and active index.
    setActive(0)
    setCollapsedProviders(new Set())
  }, [query, catalogue])
  useEffect(() => {
    if (list.current) list.current.scrollTop = 0
  }, [query])
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

  const collapseProvider = useCallback((provider: string) => {
    setCollapsedProviders((prev) => new Set([...prev, provider]))
  }, [])

  const expandProvider = useCallback((provider: string) => {
    setCollapsedProviders((prev) => { const next = new Set(prev); next.delete(provider); return next })
  }, [])

  const heading = scope === 'bot' ? 'Bot model' : 'Conversation model'
  const selected = catalogue?.models.find((row) => row.id === model)
  const label = selected?.name ?? model ?? 'Configured default'
  const compactLabel = label.split('/').pop() || label

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

  const choose = (row: ModelOption) => {
    setExperience('recentModels', [row.id, ...preferences.recentModels.filter((id) => id !== row.id)].slice(0, 8))
    onChoose(row.id)
    close()
  }

  const handleKey = useCallback((key: string): boolean => {
    if (key === 'ArrowDown' || key === 'ArrowUp') {
      byKeyboard.current = true
      const dir = key === 'ArrowDown' ? 1 : -1
      setActive((index) => {
        const pos = visibleIndices.indexOf(index)
        if (pos === -1) return nearestVisible(index, visibleIndices)
        const next = pos + dir
        if (next < 0) return visibleIndices[0] ?? 0
        if (next >= visibleIndices.length) return visibleIndices[visibleIndices.length - 1] ?? 0
        return visibleIndices[next] ?? 0
      })
      return true
    }
    if (key === 'PageDown' || key === 'PageUp') {
      byKeyboard.current = true
      const dir = key === 'PageDown' ? 1 : -1
      setActive((index) => {
        const pos = visibleIndices.indexOf(index)
        if (pos === -1) return nearestVisible(index, visibleIndices)
        const next = pos + dir * PAGE_SIZE
        if (next < 0) return visibleIndices[0] ?? 0
        if (next >= visibleIndices.length) return visibleIndices[visibleIndices.length - 1] ?? 0
        return visibleIndices[next] ?? 0
      })
      return true
    }
    if (key === 'Home') {
      byKeyboard.current = true
      setActive(visibleIndices[0] ?? 0)
      return true
    }
    if (key === 'End') {
      byKeyboard.current = true
      setActive(visibleIndices[visibleIndices.length - 1] ?? 0)
      return true
    }
    if (key === 'ArrowLeft') {
      // Collapse the active row's provider section.
      const provider = options[active]?.provider
      if (provider && !collapsedProviders.has(provider)) {
        byKeyboard.current = true
        collapseProvider(provider)
        // Move active to the first visible row at or after the current one
        // after collapse. Compute on next tick so visibleIndices reflects the update.
        setActive((index) => {
          const remaining = options.map((r, i) => ({ r, i }))
            .filter(({ r }) => r.provider !== provider)
            .map(({ i }) => i)
          return nearestVisible(index, remaining)
        })
      }
      return true
    }
    if (key === 'ArrowRight') {
      // Expand the active row's provider section if it is collapsed.
      const provider = options[active]?.provider
      if (provider && collapsedProviders.has(provider)) {
        byKeyboard.current = true
        expandProvider(provider)
      }
      return true
    }
    return false
  }, [visibleIndices, options, active, collapsedProviders, collapseProvider, expandProvider, nearestVisible])

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
          setActive(0)
          setOpen(true)
          return
        }
        setOpen(false)
        // Outside click is a blur: leave focus where the pointer landed. Escape
        // and a choice return to the button that opened the menu.
        restoreFocus()
      }}
    >
      <Button ref={trigger} slot="anchor-content" kind={scope === 'bot' ? 'plain' : 'plain-faint'} size={compact ? 'tiny' : 'medium'} className="model-trigger" isDisabled={disabled}
        data-tooltip={`Choose model · ${model ?? label}`} aria-label={`Choose model: ${label}`}
        aria-haspopup="dialog" aria-expanded={open} aria-controls={open ? id : undefined}
        data-test="model-trigger">
        {compact ? null : <Icon name="layers" slot="icon-before" />}
        <span className="model-current" aria-hidden="true">{compactLabel}</span>
        <Icon name={open ? 'carat-up' : 'carat-down'} slot="icon-after" />
      </Button>
      <div className="model-heading model-popover" id={id} data-test="model-popover">
        <span className="model-heading-title">{heading}</span>
        <Button size="small" kind="plain-faint" className="model-refresh" isDisabled={loading} onClick={() => setRevision((n) => n + 1)} data-test="model-refresh">Refresh</Button>
      </div>
      <Input ref={search} className="model-search" type="search" placeholder="Search models…" value={query}
        data-test="model-search"
        onInput={({ value }) => setQuery(value)}
        onChange={({ value }) => setQuery(value)}
        onKeyDown={({ innerEvent }) => {
          const key = (innerEvent as unknown as KeyboardEvent).key
          if (handleKey(key)) {
            innerEvent.preventDefault()
            innerEvent.stopPropagation()
          } else if (key === 'Enter') {
            innerEvent.preventDefault()
            innerEvent.stopPropagation()
            const activeRow = options[active]
            if (activeRow && !collapsedProviders.has(activeRow.provider)) choose(activeRow)
          } else if (key === 'Escape') {
            innerEvent.preventDefault()
            innerEvent.stopPropagation()
            close()
          }
        }} />
      <div className="model-notices">
        {loading && <p className="model-status" role="status" data-delayed={!showLoading || undefined}><ProgressRing mode="indeterminate" /> Loading available models…</p>}
        {problem && <Alert type="error" size="small" className="model-status" role="alert">{problem}</Alert>}
        {catalogue && <ModelWarnings warnings={catalogue.warnings} commands={catalogue.warningCommands} />}
        {!loading && options.length === 0 && <p className="model-status">{query ? 'No models match your search.' : 'No models available. Check your backend settings.'}</p>}
      </div>
      <leo-menu-section id={`${id}-list`} aria-label="Models" ref={list}>
        {options.map((row, index) => {
          const isFirstInProvider = index === 0 || options[index - 1]?.provider !== row.provider
          const isCollapsed = collapsedProviders.has(row.provider)
          return <Fragment key={row.id}>
            {isFirstInProvider && <div
              className={`model-group${isCollapsed ? ' model-group-collapsed' : ''}`}
              role="button"
              tabIndex={-1}
              aria-expanded={!isCollapsed}
              aria-label={`${row.provider} — ${isCollapsed ? 'expand' : 'collapse'}`}
              data-test={`model-group-${row.provider}`}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => {
                if (isCollapsed) expandProvider(row.provider)
                else collapseProvider(row.provider)
              }}
            >
              <span className="model-group-chevron" aria-hidden="true">
                <Icon name={isCollapsed ? 'carat-right' : 'carat-down'} />
              </span>
              {row.provider}
            </div>}
            {!isCollapsed && <leo-option id={`${id}-option-${index}`} value={row.id}
              role="option" data-index={index} data-current={row.id === model ? 'true' : undefined}
              className={index === active ? 'active' : undefined}
              onMouseDown={(event) => event.preventDefault()}
              onMouseEnter={() => setActive(index)}
              onClick={() => choose(row)}>
              <span className="model-description">
                <span className="model-name">{row.name}</span>
                <span className="model-detail">{[row.premium ? 'Premium' : null, row.contextWindow ? `${row.contextWindow.toLocaleString()} context tokens` : null, preferences.recentModels.includes(row.id) ? 'Recent' : null].filter(Boolean).join(' · ')}</span>
              </span>
              {!!row.capabilities?.length && <span className="model-capabilities" aria-label="Provider-reported capabilities">
                {row.capabilities.map((key) => {
                  const badge = CAPABILITIES[key]
                  const icon = CAPABILITY_ICONS[key]
                  if (!badge || !icon) return null
                  return <span className="model-capability" key={key} role="img" aria-label={badge[0]} data-tooltip={`${badge[1]} · Supported by this app and reported by the provider`}>
                    <Icon name={icon} />
                  </span>
                })}
              </span>}
              {row.id === catalogue?.defaultModel && <Label className="model-default" mode="outline" color="neutral">Default</Label>}
              <span className="model-check" aria-hidden="true">{row.id === model ? <Icon name="check-normal" /> : null}</span>
            </leo-option>}
          </Fragment>
        })}
      </leo-menu-section>
      <Hr />
      <p className="model-footnote">{scope === 'bot' ? 'Saved with this bot. Applies to its next message.' : 'Applies to the next message in this conversation.'} Brave Bot uses text and tools; pricing is not supplied by this catalogue.</p>
    </ButtonMenu>
  </div>
}
