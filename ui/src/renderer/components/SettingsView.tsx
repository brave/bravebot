import { useCallback, useEffect, useRef, useState } from 'react'
import type { Appearance } from '../../shared/theme'
import { Navigation, NavigationItem, type IconName } from '../nala'
import { AgentSettings } from './AgentSettings'
import { Connectors } from './Connectors'
import { GeneralSettings } from './GeneralSettings'
import { IconButton } from './IconButton'

export type SettingsPage = 'general' | 'connectors' | 'agent'

const PAGES: readonly { id: SettingsPage; label: string; icon: IconName; subtitle: string }[] = [
  { id: 'general', label: 'General', icon: 'settings', subtitle: 'How this window looks' },
  { id: 'connectors', label: 'Connectors', icon: 'plug', subtitle: 'MCP servers that give the model tools for your accounts and services' },
  { id: 'agent', label: 'Agent settings', icon: 'product-brave-leo', subtitle: 'Configuration and automation for this app' },
]

/**
 * The app's settings, in place of the chat view: a list of pages on the left and the chosen page
 * beside it.
 *
 * Unsaved hook edits are asked about before the page is left, the question the dialog used to ask
 * when it was closed.
 */
export function SettingsView({ page, onPage, onBack, session, chosen, onAppearance, onChanged }: {
  page: SettingsPage
  onPage: (page: SettingsPage) => void
  onBack: () => void
  session?: string
  chosen: Appearance
  onAppearance: (appearance: Appearance) => void
  onChanged: () => void
}): React.JSX.Element {
  const dirty = useRef(false)
  const onDirty = useCallback((value: boolean) => { dirty.current = value }, [])
  const leave = useCallback((then: () => void) => {
    if (!dirty.current || window.confirm('Discard unsaved hook changes?')) {
      dirty.current = false
      then()
    }
  }, [])
  const back = useCallback(() => leave(onBack), [leave, onBack])
  // The page shown, which follows `page` only through `leave`: a menu can change `page` directly,
  // and that is asked about the same as a click here.
  const [shown, setShown] = useState(page)
  useEffect(() => {
    if (page === shown) return
    if (!dirty.current || window.confirm('Discard unsaved hook changes?')) {
      dirty.current = false
      setShown(page)
    } else onPage(shown)
  }, [page, shown, onPage])
  /** Set by a page that has somewhere inside it to go back to, and cleared when it is at its top. */
  const [inner, setInner] = useState<(() => void) | null>(null)
  const onInner = useCallback((step: (() => void) | null) => setInner(() => step), [])
  useEffect(() => {
    const keys = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || event.defaultPrevented) return
      if (document.querySelector('[role="dialog"]')) return
      event.preventDefault()
      if (inner) inner()
      else back()
    }
    document.addEventListener('keydown', keys)
    return () => document.removeEventListener('keydown', keys)
  }, [back, inner])

  const current = PAGES.find((each) => each.id === shown) ?? PAGES[0]!

  return (
    <div className="settings-view" data-test="settings-view">
      <div className="settings-nav">
        <div className="settings-nav-titlebar" />
        <Navigation className="settings-navigation" aria-label="Settings">
          <div className="settings-nav-list">
            {PAGES.map((each) => (
              <NavigationItem key={each.id} outsideList icon={each.icon} isCurrent={each.id === shown}
                aria-current={each.id === shown ? 'page' : undefined} data-test={`settings-page-${each.id}`}
                onClick={() => { if (each.id !== shown) leave(() => onPage(each.id)) }}>
                {each.label}
              </NavigationItem>
            ))}
          </div>
          <div slot="actions" className="settings-nav-foot">
            <NavigationItem outsideList icon="arrow-left" className="settings-back" onClick={back} data-test="settings-back">
              Back to BraveBot
            </NavigationItem>
          </div>
        </Navigation>
      </div>
      <main className="settings-main" aria-labelledby="settings-title">
        <header className="settings-head">
          <div className="drag" />
          <div className="settings-crumbs">
            {inner && <IconButton icon="arrow-left" label="Back" tooltip="Back" shortcut="⎋" size="tiny" onClick={inner} data-test="settings-inner-back" />}
            <span className="settings-crumb">Settings</span>
            <span className="settings-crumb-sep" aria-hidden="true">/</span>
            <span className="settings-crumb current">{current.label}</span>
          </div>
        </header>
        <div className="settings-scroll">
          <div className="settings-page">
            <h1 id="settings-title">{current.label}</h1>
            <p className="settings-subtitle">{current.subtitle}</p>
            {shown === 'agent'
              ? <AgentSettings session={session} onChanged={onChanged} onDirty={onDirty} />
              : shown === 'connectors'
                ? <Connectors onBack={onInner} />
                : <GeneralSettings chosen={chosen} onAppearance={onAppearance} />}
          </div>
        </div>
      </main>
    </div>
  )
}
