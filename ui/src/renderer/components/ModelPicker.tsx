import { cn } from 'cn'
import { useEffect, useId, useMemo, useRef, useState } from 'react'
import type { ModelCatalogue, ModelOption } from '../../shared/protocol'
import { setExperience, useExperience } from '../experience'
import { Alert, AlertDescription } from './ui/alert'
import { Badge } from './ui/badge'
import { Button } from './ui/button'
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from './ui/command'
import { Popover, PopoverContent, PopoverTrigger } from './ui/popover'
import { Separator } from './ui/separator'
import { Spinner } from './ui/spinner'

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
  const [revision, setRevision] = useState(0)
  const trigger = useRef<HTMLButtonElement>(null)
  const search = useRef<HTMLInputElement>(null)
  const id = useId()
  const close = () => {
    setOpen(false)
    requestAnimationFrame(() => trigger.current?.focus())
  }

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
  const heading = scope === 'bot' ? 'Bot model' : 'Conversation model'
  const selected = catalogue?.models.find((row) => row.id === model)
  const label = selected?.name ?? model ?? 'Configured default'
  const compactLabel = label.split('/').pop() || label
  const choose = (row: ModelOption) => {
    setExperience('recentModels', [row.id, ...preferences.recentModels.filter((id) => id !== row.id)].slice(0, 8))
    onChoose(row.id); close()
  }

  return <Popover open={open} onOpenChange={(next) => {
    if (next) setQuery('')
    setOpen(next)
    if (!next) requestAnimationFrame(() => trigger.current?.focus())
  }}>
    <div className="model-picker min-w-[78px] max-w-[220px]">
    <PopoverTrigger asChild><Button ref={trigger} variant="outline" size="sm" className="model-trigger w-full justify-start" type="button" disabled={disabled}
      title={`Choose model · ${model ?? label}`} aria-label={`Choose model: ${label}`}
      aria-controls={open ? id : undefined}>
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
        <path d="m12 3 9 5-9 5-9-5 9-5Z M3 12l9 5 9-5 M3 16l9 5 9-5" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
      <span className="model-current min-w-0 flex-1 truncate text-left text-[10.5px]">{compactLabel}</span>
    </Button></PopoverTrigger>
    </div>
    <PopoverContent id={id} className="model-popover w-[min(380px,66vw)] p-0" aria-label={heading} side="top" align="end"
      onOpenAutoFocus={(event) => { event.preventDefault(); search.current?.focus() }}>
      <Command shouldFilter={false} className="model-command">
        <div className="model-heading mb-2 flex items-center justify-between gap-2 px-px"><strong className="text-xs font-semibold">{heading}</strong>
          <Button variant="ghost" size="sm" type="button" className="model-refresh text-primary" disabled={loading} onClick={() => setRevision((n) => n + 1)}>Refresh</Button>
        </div>
        <CommandInput ref={search} className="model-search bg-muted" placeholder="Search models…" value={query}
          role="combobox" aria-label="Search models" onValueChange={setQuery}
          onKeyDown={(event) => { if (event.key === 'Enter' && options.length === 0) event.preventDefault() }} />
        {loading && <div className="model-status mt-2 text-[11px] text-muted-foreground" role="status"><Spinner data-icon="inline-start" /> Loading available models…</div>}
        {problem && <Alert variant="destructive" className="model-status mt-2"><AlertDescription>{problem}</AlertDescription></Alert>}
        {catalogue?.warnings.map((warning) => <Alert className="model-status mt-2" key={warning}><AlertDescription>{warning}</AlertDescription></Alert>)}
        <CommandList id={`${id}-list`} className="model-options mt-2" aria-label="Models">
          <CommandEmpty className="model-status mt-2 text-[11px] text-muted-foreground">{query ? 'No models match your search.' : 'No models available. Check your backend settings.'}</CommandEmpty>
          <CommandGroup>
          {options.map((row) => <CommandItem key={row.id} value={row.id} role="option"
          data-current={row.id === model}
          className="model-option gap-2 rounded-md px-2 py-2 data-[selected=true]:bg-primary/10 data-[selected=true]:text-foreground"
          onSelect={() => choose(row)}>
          <span className="model-check w-3.5 shrink-0 text-primary" aria-hidden="true">{row.id === model ? '✓' : ''}</span>
          <span className="model-description flex min-w-0 flex-1 flex-col gap-0.5"><span className="model-name truncate text-[12.5px] font-medium">{row.name}</span>
            <span className="model-detail text-[10.5px] text-muted-foreground">{row.provider}{row.premium ? ' · Premium' : ''}{row.contextWindow ? ` · ${row.contextWindow.toLocaleString()} context tokens` : ''}{preferences.recentModels.includes(row.id) ? ' · Recent' : ''}</span>
            {!!row.capabilities?.length && <span className="model-capabilities mt-1 flex flex-wrap gap-1" aria-label="Provider-reported capabilities">
              {row.capabilities.filter((key) => ['text', 'tools'].includes(key)).map((key) => {
                const badge = CAPABILITIES[key]
                return badge ? <Badge variant="outline" className="model-capability rounded-[4px] border-border bg-card px-1 py-0 text-[9px] leading-[14px] text-muted-foreground" key={key} title={`${badge[1]} · Supported by this app and reported by the provider`}>
                  {badge[0]}
                </Badge> : null
              })}
            </span>}
          </span>
          {row.id === catalogue?.defaultModel && <Badge variant="secondary" className="model-default rounded-[4px] border-border bg-card px-1.5 py-0.5 text-[10px] text-muted-foreground">Default</Badge>}
        </CommandItem>)}
          </CommandGroup>
        </CommandList>
        <Separator />
        <div className="model-footnote mt-2 border-t border-border pt-2 text-[10px] text-muted-foreground">{scope === 'bot' ? 'Saved with this bot. Applies to its next message.' : 'Applies to the next message in this conversation.'}</div>
        <p className="model-footnote mt-2 border-t border-border pt-2 text-[10px] text-muted-foreground">Brave Bot uses text and tools. Other provider capabilities, such as image or audio generation, are not available here. Pricing is not supplied by this catalogue.</p>
      </Command>
    </PopoverContent>
  </Popover>
}
