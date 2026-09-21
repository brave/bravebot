import { useSyncExternalStore } from 'react'
import { EMPTY_CONVERSATION, parseExperience, type ConversationPreferences, type Experience } from '../shared/experience'

let state = parseExperience(null)
let started = false
const changed = new Set<string>()
const listeners = new Set<() => void>()
const emit = () => listeners.forEach((listener) => listener())
let saveError = ''

function start() {
  if (started) return
  started = true
  void window.bravebot.readExperience().then((saved) => {
    state = { ...saved, conversations: { ...saved.conversations, ...state.conversations },
      density: changed.has('density') ? state.density : saved.density,
      recentModels: changed.has('recentModels') ? state.recentModels : saved.recentModels }
    emit()
  }).catch(() => { saveError = 'Saved preferences could not be loaded.'; emit() })
}

export function useExperience(): Experience {
  return useSyncExternalStore((listener) => { listeners.add(listener); start(); return () => { listeners.delete(listener) } }, () => state)
}

export const experienceError = () => saveError
export const conversationPreferences = (key: string) => state.conversations[key] ?? EMPTY_CONVERSATION

export function setConversation(key: string, patch: Partial<ConversationPreferences>) {
  const next = { ...conversationPreferences(key), ...patch }
  state = { ...state, conversations: { ...state.conversations, [key]: next } }
  persist(key, next)
}

export function setExperience<K extends 'density' | 'recentModels'>(key: K, value: Experience[K]) {
  state = { ...state, [key]: value }
  persist(key, value)
}

function persist(key: string, value: unknown) {
  changed.add(key)
  emit()
  void window.bravebot.writeExperience(key, value).then(() => {
    if (saveError) { saveError = ''; emit() }
  }).catch(() => { saveError = 'Could not save your draft or preferences. Keep this window open and check available disk space.'; emit() })
}
