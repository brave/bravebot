import { useEffect, useRef, useState } from 'react'
import { hasChanges, modelShown, ordered, providerShown, resetProvider, setModelShown, setOrder, setProviderShown } from '../../shared/model-preferences'
import type { ModelOption } from '../../shared/protocol'
import { setExperience, useExperienceValue } from '../experience'
import { useDelayedFlag } from '../hooks'
import { Alert, Button, Toggle } from '../nala'
import { IconButton } from './IconButton'
import { Skeleton } from './Skeleton'

/** `ids` with the one at `from` moved to `to`. */
function moved(ids: readonly string[], from: number, to: number): string[] {
  const next = [...ids]
  const [id] = next.splice(from, 1)
  if (id !== undefined) next.splice(to, 0, id)
  return next
}

/**
 * One service's models in Settings › Providers: whether the service appears in the model picker,
 * each model shown or hidden, the order the picker lists them in, and a reset that forgets all of
 * it. Signing in is the account row's business, so a reset leaves it as it was.
 *
 * A row is moved by dragging its handle, or with the handle focused by Alt with the up or down
 * arrow. The rows keep their height while one is dragged, so nothing under the list moves, and the
 * new place is said through a live region.
 *
 * `models` is null while the catalogue loads.
 */
export function ProviderModels({ providerKey, name, models, problem, onRetry }: {
  providerKey: string
  name: string
  models: ModelOption[] | null
  problem: string | null
  onRetry: () => void
}): React.JSX.Element {
  const preferences = useExperienceValue((experience) => experience.providers)
  const preference = preferences[providerKey]
  const shown = providerShown(preferences, providerKey)
  const rows = models ? ordered(models, preference) : []
  const loading = models === null && !problem
  const showLoading = useDelayedFlag(loading)
  const [said, setSaid] = useState('')
  /** The row being dragged, where it started, and where it would land now. */
  const [drag, setDrag] = useState<{ id: string; from: number; to: number; startY: number; height: number } | null>(null)
  const list = useRef<HTMLUListElement>(null)
  const pendingFocus = useRef<string | null>(null)

  const save = (next: typeof preferences) => setExperience('providers', next)
  const ids = rows.map((row) => row.id)
  const shownIds = drag ? moved(ids, drag.from, drag.to) : ids
  const byId = new Map(rows.map((row) => [row.id, row]))

  const commit = (next: readonly string[], id: string) => {
    save(setOrder(preferences, providerKey, next))
    const row = byId.get(id)
    setSaid(`${row?.name ?? id} moved to position ${next.indexOf(id) + 1} of ${next.length}.`)
  }

  useEffect(() => {
    const id = pendingFocus.current
    if (!id) return
    pendingFocus.current = null
    list.current?.querySelector<HTMLElement>(`[data-model-id="${CSS.escape(id)}"] .provider-model-handle leo-button`)?.focus()
  })

  return <div className={`provider-models${shown ? '' : ' provider-hidden'}`} data-test="provider-models">
    <div className="provider-models-row">
      <span className="provider-models-label" id={`${providerKey}-shown`}>Show in model picker</span>
      <Toggle size="small" checked={shown} aria-labelledby={`${providerKey}-shown`} data-test="provider-shown"
        onChange={({ checked }) => save(setProviderShown(preferences, providerKey, checked))} />
    </div>
    <h4 className="provider-models-title">Models</h4>
    {loading && <Skeleton label="Loading models…" count={3} delayed={!showLoading} />}
    {problem && <div className="provider-models-problem">
      <Alert type="error" size="small" role="alert">{problem}</Alert>
      <Button size="small" kind="outline" onClick={onRetry}>Try again</Button>
    </div>}
    {models && rows.length === 0 && <p className="settings-caption" data-test="provider-models-empty">{name} lists no models.</p>}
    {rows.length > 0 && <ul ref={list} className="provider-model-list" aria-label={`${name} models`} data-test="provider-model-list">
      {shownIds.map((id, index) => {
        const row = byId.get(id)
        if (!row) return null
        const visible = modelShown(preferences, providerKey, id)
        return <li key={id} className={`provider-model${drag?.id === id ? ' dragging' : ''}`} data-model-id={id} data-test={`provider-model-${id}`}>
          <span className="provider-model-handle"
            onPointerDown={(event) => {
              if (event.button !== 0) return
              const item = (event.currentTarget as HTMLElement).closest('li')
              event.currentTarget.setPointerCapture(event.pointerId)
              setDrag({ id, from: index, to: index, startY: event.clientY, height: item?.getBoundingClientRect().height || 1 })
            }}
            onPointerMove={(event) => {
              if (!drag || drag.id !== id) return
              const to = Math.min(ids.length - 1, Math.max(0, drag.from + Math.round((event.clientY - drag.startY) / drag.height)))
              if (to !== drag.to) setDrag({ ...drag, to })
            }}
            onPointerUp={() => {
              if (!drag) return
              const next = moved(ids, drag.from, drag.to)
              setDrag(null)
              if (drag.from !== drag.to) commit(next, drag.id)
            }}
            onPointerCancel={() => setDrag(null)}
            onKeyDown={(event) => {
              if (!event.altKey || (event.key !== 'ArrowUp' && event.key !== 'ArrowDown')) return
              event.preventDefault()
              const to = index + (event.key === 'ArrowUp' ? -1 : 1)
              if (to < 0 || to >= ids.length) return
              pendingFocus.current = id
              commit(moved(ids, index, to), id)
            }}>
            <IconButton icon="drag" label={`Move ${row.name}`} tooltip="Drag, or press Alt with the up or down arrow, to move" size="small" data-test="provider-model-move" />
          </span>
          <span className="provider-model-name" data-tooltip={row.name}>{row.name}</span>
          <Toggle size="small" checked={visible} aria-label={`Show ${row.name} in the model picker`} data-test="provider-model-shown"
            onChange={({ checked }) => save(setModelShown(preferences, providerKey, id, checked))} />
        </li>
      })}
    </ul>}
    <p className="visually-hidden" aria-live="polite" data-test="provider-models-said">{said}</p>
    <div className="settings-actions">
      <Button size="medium" kind="outline" isDisabled={!hasChanges(preferences, providerKey)} data-test="provider-reset"
        onClick={() => { save(resetProvider(preferences, providerKey)); setSaid(`${name} is back to its default models and order.`) }}>
        Reset to defaults
      </Button>
    </div>
  </div>
}
