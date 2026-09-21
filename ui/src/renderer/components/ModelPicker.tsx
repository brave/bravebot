import { useEffect, useId, useMemo, useRef, useState } from 'react'
import type { ModelCatalogue, ModelOption } from '../../shared/protocol'
import { setExperience, useExperience } from '../experience'

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

export function ModelPicker({ model, disabled, onChoose, scope = 'conversation', session }: {
  model: string | null
  scope?: 'conversation' | 'bot'
  session?: string
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
  const root = useRef<HTMLDivElement>(null)
  const trigger = useRef<HTMLButtonElement>(null)
  const search = useRef<HTMLInputElement>(null)
  const list = useRef<HTMLDivElement>(null)
  const id = useId()
  const close = () => { setOpen(false); trigger.current?.focus() }

  useEffect(() => {
    if (!open) return
    let gone = false
    setLoading(true)
    setProblem(null)
    search.current?.focus()
    void window.bravebot.request<ModelCatalogue>('models.list', { session }).then((answer) => {
      if (gone) return
      if (answer.error) setProblem(answer.error.message)
      else setCatalogue(answer.ok ?? null)
    }).catch(() => {
      if (!gone) setProblem('Could not load models. Try again.')
    }).finally(() => { if (!gone) setLoading(false) })
    return () => { gone = true }
  }, [open, revision, session])

  useEffect(() => {
    if (!open) return
    const outside = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false)
    }
    document.addEventListener('pointerdown', outside)
    return () => document.removeEventListener('pointerdown', outside)
  }, [open])

  useEffect(() => { if (disabled) setOpen(false) }, [disabled])

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
  useEffect(() => { setActive(0) }, [query, catalogue])
  useEffect(() => {
    list.current?.querySelector(`[data-index="${active}"]`)?.scrollIntoView({ block: 'nearest' })
  }, [active])

  const heading = scope === 'bot' ? 'Bot model' : 'Conversation model'
  const selected = catalogue?.models.find((row) => row.id === model)
  const label = selected?.name ?? model ?? 'Configured default'
  const compactLabel = label.split('/').pop() || label
  const choose = (row: ModelOption) => {
    setExperience('recentModels', [row.id, ...preferences.recentModels.filter((id) => id !== row.id)].slice(0, 8))
    onChoose(row.id); close()
  }

  return <div className="model-picker" ref={root} onBlur={(event) => {
    if (!event.currentTarget.contains(event.relatedTarget)) setOpen(false)
  }}>
    <button ref={trigger} className="model-trigger" type="button" disabled={disabled}
      title={`Choose model · ${model ?? label}`} aria-label={`Choose model: ${label}`}
      aria-haspopup="dialog" aria-expanded={open} aria-controls={open ? id : undefined}
      onClick={() => { setQuery(''); setActive(0); setOpen((value) => !value) }}>
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
        <path d="m12 3 9 5-9 5-9-5 9-5Z M3 12l9 5 9-5 M3 16l9 5 9-5" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
      <span className="model-current" aria-hidden="true">{compactLabel}</span>
    </button>
    {open && <div id={id} className="model-popover" role="dialog" aria-label={heading}
      onKeyDown={(event) => {
        if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close() }
      }}>
      <div className="model-heading"><strong>{heading}</strong>
        <button type="button" className="model-refresh" disabled={loading} onClick={() => setRevision((n) => n + 1)}>Refresh</button>
      </div>
      <input ref={search} className="model-search" type="search" placeholder="Search models…" value={query}
        role="combobox" aria-label="Search models" aria-autocomplete="list" aria-expanded="true"
        aria-controls={`${id}-list`} aria-activedescendant={options[active] ? `${id}-option-${active}` : undefined}
        onChange={(event) => setQuery(event.target.value)} onKeyDown={(event) => {
          if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
            event.preventDefault()
            setActive((index) => Math.max(0, Math.min(options.length - 1, index + (event.key === 'ArrowDown' ? 1 : -1))))
          } else if (event.key === 'Enter') {
            event.preventDefault()
            if (options[active]) choose(options[active])
          }
        }} />
      {loading && <p className="model-status" role="status">Loading available models…</p>}
      {problem && <p className="model-status" role="alert">{problem}</p>}
      {catalogue?.warnings.map((warning) => <p className="model-status" key={warning}>{warning}</p>)}
      <div id={`${id}-list`} className="model-options" role="listbox" aria-label="Models" ref={list}>
        {options.map((row, index) => <div key={row.id} id={`${id}-option-${index}`} role="option"
          aria-selected={row.id === model} data-index={index}
          className={`model-option ${index === active ? 'active' : ''}`}
          onMouseDown={(event) => event.preventDefault()} onMouseEnter={() => setActive(index)} onClick={() => choose(row)}>
          <span className="model-check" aria-hidden="true">{row.id === model ? '✓' : ''}</span>
          <span className="model-description"><span className="model-name">{row.name}</span>
            <span className="model-detail">{row.provider}{row.premium ? ' · Premium' : ''}{row.contextWindow ? ` · ${row.contextWindow.toLocaleString()} context tokens` : ''}{preferences.recentModels.includes(row.id) ? ' · Recent' : ''}</span>
            {!!row.capabilities?.length && <span className="model-capabilities" aria-label="Provider-reported capabilities">
              {row.capabilities.filter((key) => ['text', 'tools'].includes(key)).map((key) => {
                const badge = CAPABILITIES[key]
                return badge ? <span className="model-capability" key={key} title={`${badge[1]} · Supported by this app and reported by the provider`}>
                  {badge[0]}
                </span> : null
              })}
            </span>}
          </span>
          {row.id === catalogue?.defaultModel && <span className="model-default">Default</span>}
        </div>)}
      </div>
      {!loading && options.length === 0 && <p className="model-status">{query ? 'No models match your search.' : 'No models available. Check your backend settings.'}</p>}
      <div className="model-footnote">{scope === 'bot' ? 'Saved with this bot. Applies to its next message.' : 'Applies to the next message in this conversation.'}</div>
      <p className="model-footnote">Brave Bot uses text and tools. Other provider capabilities, such as image or audio generation, are not available here. Pricing is not supplied by this catalogue.</p>
    </div>}
  </div>
}
