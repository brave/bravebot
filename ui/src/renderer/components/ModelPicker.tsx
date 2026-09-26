import { useEffect, useMemo, useRef, useState } from 'react'
import type { ModelCatalogue, ModelOption } from '../../shared/protocol'
import { setExperience, useExperience } from '../experience'
import { cn } from '@/lib/utils'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from '@/components/ui/command'
import {
  Popover,
  PopoverContent,
  PopoverHeader,
  PopoverTitle,
  PopoverTrigger,
} from '@/components/ui/popover'
import { Layers } from 'lucide-react'

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

/** Whatever the catalogue has to say for itself — loading, refusing, or warning. */
const STATUS = 'model-status mt-2 text-[11px] text-muted-foreground'

/**
 * The two things this popover says about the choice rather than about a model, ruled off from the
 * list above them: a setting nobody asked about should not be mistaken for another option.
 */
const FOOTNOTE = 'model-footnote mt-2 border-t border-border pt-2 text-[11px] text-muted-foreground'

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
  const triggerRef = useRef<HTMLButtonElement | null>(null)

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

  useEffect(() => { if (disabled) setOpen(false) }, [disabled])

  const options = useMemo(() => {
    const rows = [...(catalogue?.models ?? [])]
    if (model && !rows.some((row) => row.id === model)) {
      rows.unshift({ id: model, name: model, provider: 'Current selection', premium: false, contextWindow: null })
    }
    const words = query.toLowerCase().trim().split(/\s+/).filter(Boolean)
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
    onChoose(row.id)
    setOpen(false)
  }

  return (
    <div className="model-picker relative min-w-[70px] max-w-40 flex-[0_1_auto] self-end">
      <Popover open={open} onOpenChange={(next) => {
        if (disabled) return
        setQuery('')
        setOpen(next)
        if (!next) {
          // Our Escape handler closes via setOpen; restore focus the way the old popover did.
          queueMicrotask(() => triggerRef.current?.focus())
        }
      }}>
        <PopoverTrigger
          disabled={disabled}
          render={
            <button
              ref={triggerRef}
              className="model-trigger flex h-[38px] w-full cursor-pointer items-center gap-1.5 rounded-lg border border-border bg-background px-2 text-muted-foreground hover:border-primary hover:text-primary aria-expanded:border-primary aria-expanded:text-primary disabled:cursor-default focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
              type="button"
              disabled={disabled}
              title={`Choose model · ${model ?? label}`}
              aria-label={`Choose model: ${label}`}
            />
          }
        >
          <Layers className="shrink-0" aria-hidden="true" />
          {/* The name only, and ellipsised: the composer's width belongs to what somebody is
              typing, and the whole identifier is in the trigger's title either way. */}
          <span className="model-current truncate text-xs" aria-hidden="true">{compactLabel}</span>
        </PopoverTrigger>
        {open && (
        <PopoverContent
          className="model-popover w-[min(360px,60vw)] max-h-[min(420px,65vh)] p-3"
          side="top"
          align="start"
          sideOffset={10}
          aria-label={heading}
          onKeyDown={(event) => {
            // Always dismiss — do not let cmdk clear the query on the first Escape.
            if (event.key === 'Escape') {
              event.preventDefault()
              event.stopPropagation()
              setOpen(false)
            }
          }}
        >
          <PopoverHeader className="model-heading flex-row items-center justify-between gap-3">
            <PopoverTitle>
              <strong className="text-xs font-semibold">{heading}</strong>
            </PopoverTitle>
            <Button
              type="button"
              variant="ghost"
              size="xs"
              className="model-refresh text-[11px] text-primary focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
              disabled={loading}
              onClick={() => setRevision((n) => n + 1)}
            >
              Refresh
            </Button>
          </PopoverHeader>
          <Command shouldFilter={false} className="bg-transparent p-0">
            <CommandInput
              className="model-search"
              placeholder="Search models…"
              value={query}
              onValueChange={setQuery}
              aria-label="Search models"
              onKeyDown={(event) => {
                if (event.key === 'Enter' && options[0]) {
                  event.preventDefault()
                  choose(options[0])
                }
              }}
            />
            {loading && <p className={STATUS} role="status">Loading available models…</p>}
            {problem && <p className={STATUS} role="alert">{problem}</p>}
            {catalogue?.warnings.map((warning) => <p className={STATUS} key={warning}>{warning}</p>)}
            <CommandList className="model-options max-h-none min-h-0">
              <CommandGroup>
                {options.map((row) => (
                  <CommandItem
                    key={row.id}
                    value={row.id}
                    className="model-option cursor-pointer px-1.5 py-2"
                    onSelect={() => choose(row)}
                  >
                    <span className="model-check w-3.5 shrink-0 text-primary" aria-hidden="true">{row.id === model ? '✓' : ''}</span>
                    <span className="model-description flex min-w-0 flex-1 flex-col gap-[3px]">
                      <span className="model-name truncate text-[13px] group-aria-selected/command-item:text-primary">{row.name}</span>
                      <span className="model-detail text-[11px] text-muted-foreground">
                        {row.provider}
                        {row.premium ? ' · Premium' : ''}
                        {row.contextWindow ? ` · ${row.contextWindow.toLocaleString()} context tokens` : ''}
                        {preferences.recentModels.includes(row.id) ? ' · Recent' : ''}
                      </span>
                      {!!row.capabilities?.length && (
                        <span className="model-capabilities mt-[3px] flex flex-wrap gap-1" aria-label="Provider-reported capabilities">
                          {row.capabilities.filter((key) => ['text', 'tools'].includes(key)).map((key) => {
                            const badge = CAPABILITIES[key]
                            return badge ? (
                              <Badge
                                key={key}
                                variant="outline"
                                className="model-capability h-auto rounded-[4px] px-1 text-[11px] leading-[14px] text-muted-foreground"
                                title={`${badge[1]} · Supported by this app and reported by the provider`}
                              >
                                {badge[0]}
                              </Badge>
                            ) : null
                          })}
                        </span>
                      )}
                    </span>
                    {row.id === catalogue?.defaultModel && <span className="model-default shrink-0 rounded-[4px] border border-border px-[5px] py-0.5 text-[10px] text-muted-foreground">Default</span>}
                  </CommandItem>
                ))}
              </CommandGroup>
              {!loading && options.length === 0 && (
                <CommandEmpty className={cn(STATUS, 'py-0 text-left')}>
                  {query ? 'No models match your search.' : 'No models available. Check your backend settings.'}
                </CommandEmpty>
              )}
            </CommandList>
          </Command>
          <div className={FOOTNOTE}>{scope === 'bot' ? 'Saved with this bot. Applies to its next message.' : 'Applies to the next message in this conversation.'}</div>
          <p className={cn(FOOTNOTE, 'm-0 mt-2')}>Brave Bot uses text and tools. Other provider capabilities, such as image or audio generation, are not available here. Pricing is not supplied by this catalogue.</p>
        </PopoverContent>
        )}
      </Popover>
    </div>
  )
}
