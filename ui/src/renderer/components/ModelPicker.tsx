import { useEffect, useId, useMemo, useRef, useState } from 'react'
import type { ModelCatalogue, ModelOption } from '../../shared/protocol'
import { setExperience, useExperience } from '../experience'
import { Alert, Button, ButtonMenu, Hr, Icon, Input, Label, ProgressRing } from '../nala'

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
  const [problem, setProblem] = useState<string | null>(null)
  const [query, setQuery] = useState('')
  const [active, setActive] = useState(0)
  const [revision, setRevision] = useState(0)
  const trigger = useRef<HTMLElement>(null)
  const search = useRef<HTMLElement>(null)
  const list = useRef<HTMLElement>(null)
  const byKeyboard = useRef(false)
  const shutReason = useRef<string>('explicit')
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
      else setCatalogue(answer.ok ?? null)
    }).catch(() => {
      if (!gone) setProblem('Could not load models. Try again.')
    }).finally(() => { if (!gone) setLoading(false) })
    return () => { gone = true }
  }, [open, revision, session])

  const options = useMemo(() => {
    const rows = [...(catalogue?.models ?? [])]
    if (model && !rows.some((row) => row.id === model)) {
      rows.unshift({ id: model, name: model, provider: 'Current selection', premium: false, contextWindow: null })
    }
    const words = query.toLowerCase().trim().split(/\s+/)
    return rows.filter((row) => {
      const capabilities = (row.capabilities ?? []).map((key) =>
        `${key} ${CAPABILITIES[key]?.join(' ') ?? ''}`).join(' ')
      const searchable = `${row.name} ${row.id} ${row.provider} ${capabilities}`.toLowerCase()
      return words.every((word) => searchable.includes(word))
    }).sort((a, b) => {
      const rank = (row: ModelOption) => row.id === model ? -2 : preferences.recentModels.includes(row.id) ? preferences.recentModels.indexOf(row.id) : 100
      return rank(a) - rank(b)
    })
  }, [catalogue, model, query, preferences.recentModels])

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
  useEffect(() => { setActive(0) }, [query, catalogue])
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

  return <div className="model-picker">
    <ButtonMenu
      className="model-menu"
      isOpen={open && !disabled}
      placement="top-end"
      positionStrategy="fixed"
      onClose={(detail) => { shutReason.current = detail.reason }}
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
        if (shutReason.current !== 'blur') trigger.current?.focus()
        shutReason.current = 'explicit'
      }}
    >
      <Button ref={trigger} slot="anchor-content" kind={scope === 'bot' ? 'plain' : 'plain-faint'} size={compact ? 'tiny' : 'medium'} className="model-trigger" isDisabled={disabled}
        title={`Choose model · ${model ?? label}`} aria-label={`Choose model: ${label}`}
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
          if (key === 'ArrowDown' || key === 'ArrowUp') {
            innerEvent.preventDefault()
            innerEvent.stopPropagation()
            byKeyboard.current = true
            setActive((index) => Math.max(0, Math.min(options.length - 1, index + (key === 'ArrowDown' ? 1 : -1))))
          } else if (key === 'Enter') {
            innerEvent.preventDefault()
            innerEvent.stopPropagation()
            if (options[active]) choose(options[active])
          } else if (key === 'Escape') {
            innerEvent.preventDefault()
            innerEvent.stopPropagation()
            close()
          }
        }} />
      <div className="model-notices">
        {loading && <p className="model-status" role="status"><ProgressRing mode="indeterminate" /> Loading available models…</p>}
        {problem && <Alert type="error" size="small" className="model-status" role="alert">{problem}</Alert>}
        {catalogue?.warnings.map((warning) => <p className="model-status" key={warning}>{warning}</p>)}
        {!loading && options.length === 0 && <p className="model-status">{query ? 'No models match your search.' : 'No models available. Check your backend settings.'}</p>}
      </div>
      <leo-menu-section id={`${id}-list`} aria-label="Models" ref={list}>
        {options.map((row, index) => <leo-option key={row.id} id={`${id}-option-${index}`} value={row.id}
          role="option" data-index={index} data-current={row.id === model ? 'true' : undefined}
          className={index === active ? 'active' : undefined}
          onMouseDown={(event) => event.preventDefault()}
          onMouseEnter={() => setActive(index)}
          onClick={() => choose(row)}>
          <span className="model-check" aria-hidden="true">{row.id === model ? <Icon name="check-normal" /> : null}</span>
          <span className="model-description">
            <span className="model-name">{row.name}</span>
            <span className="model-detail">{row.provider}{row.premium ? ' · Premium' : ''}{row.contextWindow ? ` · ${row.contextWindow.toLocaleString()} context tokens` : ''}{preferences.recentModels.includes(row.id) ? ' · Recent' : ''}</span>
            {!!row.capabilities?.length && <span className="model-capabilities" aria-label="Provider-reported capabilities">
              {row.capabilities.filter((key) => ['text', 'tools'].includes(key)).map((key) => {
                const badge = CAPABILITIES[key]
                return badge ? <Label className="model-capability" key={key} mode="outline" color="neutral">
                  <span title={`${badge[1]} · Supported by this app and reported by the provider`}>{badge[0]}</span>
                </Label> : null
              })}
            </span>}
          </span>
          {row.id === catalogue?.defaultModel && <Label className="model-default" mode="outline" color="neutral">Default</Label>}
        </leo-option>)}
      </leo-menu-section>
      <Hr />
      <p className="model-footnote">{scope === 'bot' ? 'Saved with this bot. Applies to its next message.' : 'Applies to the next message in this conversation.'}</p>
      <Hr />
      <p className="model-footnote">Brave Bot uses text and tools. Other provider capabilities, such as image or audio generation, are not available here. Pricing is not supplied by this catalogue.</p>
    </ButtonMenu>
  </div>
}
