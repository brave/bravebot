/**
 * What a person chose about each model service in Settings › Providers: whether its models appear
 * in the model picker, which of them are hidden, and the order they are listed in.
 *
 * Kept by the desktop window alone, in its experience file, against the service keys the bridge
 * names in `models.list` (`providerKey`) and that `accountProviderKey` names for an account row.
 * Nothing here is read by the agent: hiding a model only takes it off the window's lists.
 */

export interface ProviderPreference {
  /** The service's models are left out of the model picker. */
  hidden?: true
  /** Models of the service left out of the model picker, by id. */
  hiddenModels?: string[]
  /** The ids a person put in order, first first. Models not named here follow, in catalogue order. */
  order?: string[]
}

export type ProviderPreferences = Record<string, ProviderPreference>

export const MAX_PROVIDERS = 200
export const MAX_MODELS_PER_PROVIDER = 2000
const MAX_ID = 1000

const isId = (value: unknown): value is string => typeof value === 'string' && value.length > 0 && value.length < MAX_ID

function ids(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined
  const kept = [...new Set(value.filter(isId))].slice(0, MAX_MODELS_PER_PROVIDER)
  return kept.length > 0 ? kept : undefined
}

/** Read what a file or a window sent, keeping only well-formed entries. */
export function parseProviderPreferences(value: unknown): ProviderPreferences {
  const parsed: ProviderPreferences = Object.create(null) as ProviderPreferences
  if (!value || typeof value !== 'object' || Array.isArray(value)) return parsed
  for (const [key, entry] of Object.entries(value).slice(0, MAX_PROVIDERS)) {
    if (!isId(key) || !entry || typeof entry !== 'object') continue
    const raw = entry as Record<string, unknown>
    const preference: ProviderPreference = {}
    if (raw.hidden === true) preference.hidden = true
    const hiddenModels = ids(raw.hiddenModels)
    if (hiddenModels) preference.hiddenModels = hiddenModels
    const order = ids(raw.order)
    if (order) preference.order = order
    if (!isEmpty(preference)) parsed[key] = preference
  }
  return parsed
}

const isEmpty = (preference: ProviderPreference | undefined) =>
  !preference || (!preference.hidden && !preference.hiddenModels?.length && !preference.order?.length)

/** Whether a person changed anything about `key`, so there is something to reset. */
export const hasChanges = (preferences: ProviderPreferences, key: string) => !isEmpty(preferences[key])

function withEntry(preferences: ProviderPreferences, key: string, entry: ProviderPreference): ProviderPreferences {
  const { [key]: _old, ...rest } = preferences
  return isEmpty(entry) ? rest : { ...rest, [key]: entry }
}

/** Show or hide every model of `key` in the model picker. */
export function setProviderShown(preferences: ProviderPreferences, key: string, shown: boolean): ProviderPreferences {
  const { hidden: _hidden, ...entry } = preferences[key] ?? {}
  return withEntry(preferences, key, shown ? entry : { ...entry, hidden: true })
}

/** Show or hide one model of `key` in the model picker. */
export function setModelShown(preferences: ProviderPreferences, key: string, model: string, shown: boolean): ProviderPreferences {
  const entry = preferences[key] ?? {}
  const others = (entry.hiddenModels ?? []).filter((id) => id !== model)
  const hiddenModels = shown ? others : [...others, model]
  return withEntry(preferences, key, { ...entry, hiddenModels: hiddenModels.length ? hiddenModels : undefined })
}

/** Put the models of `key` in the order `ids` lists them. */
export function setOrder(preferences: ProviderPreferences, key: string, ordered: readonly string[]): ProviderPreferences {
  const entry = preferences[key] ?? {}
  return withEntry(preferences, key, { ...entry, order: ordered.length ? [...ordered] : undefined })
}

/** Forget everything a person chose about `key`. Signing in is kept elsewhere and is not touched. */
export function resetProvider(preferences: ProviderPreferences, key: string): ProviderPreferences {
  return withEntry(preferences, key, {})
}

/** `models` in the order a person chose for the service, the ones not named after, in their order. */
export function ordered<T extends { id: string }>(models: readonly T[], preference: ProviderPreference | undefined): T[] {
  const rank = new Map((preference?.order ?? []).map((id, index) => [id, index]))
  return models
    .map((model, index) => ({ model, index, rank: rank.get(model.id) ?? Number.POSITIVE_INFINITY }))
    .sort((a, b) => a.rank - b.rank || a.index - b.index)
    .map((entry) => entry.model)
}

/** Whether a person left `model` of `key` showing in the picker. */
export const modelShown = (preferences: ProviderPreferences, key: string, model: string) =>
  !preferences[key]?.hiddenModels?.includes(model)

/** Whether a person left the service `key` showing in the picker. */
export const providerShown = (preferences: ProviderPreferences, key: string) => !preferences[key]?.hidden

export interface Arranged<T> {
  /** The services that have a model to show, in the order their first model came in `models`. */
  providers: string[]
  /** The models to show, grouped by service in that order, each group in its chosen order. */
  models: T[]
}

/**
 * The picker's rows: every model whose service and whose own row a person left showing, grouped by
 * service and put in the order they chose. `current`, the model in use, stays whatever was chosen,
 * so a model a person hid while it was selected does not vanish from under the trigger.
 */
export function arrange<T extends { id: string; providerKey: string }>(
  models: readonly T[],
  preferences: ProviderPreferences,
  current: string | null,
): Arranged<T> {
  const groups = new Map<string, T[]>()
  for (const model of models) {
    const shown = model.id === current
      || (providerShown(preferences, model.providerKey) && modelShown(preferences, model.providerKey, model.id))
    if (!shown) continue
    const group = groups.get(model.providerKey)
    if (group) group.push(model)
    else groups.set(model.providerKey, [model])
  }
  const providers = [...groups.keys()]
  return { providers, models: providers.flatMap((key) => ordered(groups.get(key) ?? [], preferences[key])) }
}
