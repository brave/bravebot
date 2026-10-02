import { useCallback, useEffect, useRef } from 'react'
import type { Appearance } from '../../shared/theme'
import { Navigation, NavigationItem, type IconName } from '../nala'
import { AgentSettings } from './AgentSettings'
import { GeneralSettings } from './GeneralSettings'
import { IconButton } from './IconButton'

export type SettingsPage = 'general' | 'agent'

const PAGES: readonly { id: SettingsPage; label: string; icon: IconName; subtitle: string }[] = [
  { id: 'general', label: 'General', icon: 'settings', subtitle: 'How this window looks' },
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
  useEffect(() => {
    const keys = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || event.defaultPrevented) return
      if (document.querySelector('[role="dialog"]')) return
      event.preventDefault()
      back()
    }
    document.addEventListener('keydown', keys)
    return () => document.removeEventListener('keydown', keys)
  }, [back])

  const current = PAGES.find((each) => each.id === page) ?? PAGES[0]!

  return (
    <div className="settings-view" data-test="settings-view">
      <div className="settings-nav">
        <div className="settings-nav-titlebar" />
        <Navigation className="settings-navigation" aria-label="Settings">
          <div className="settings-nav-list">
            {PAGES.map((each) => (
              <NavigationItem key={each.id} outsideList icon={each.icon} isCurrent={each.id === page}
                aria-current={each.id === page ? 'page' : undefined} data-test={`settings-page-${each.id}`}
                onClick={() => { if (each.id !== page) leave(() => onPage(each.id)) }}>
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
            <IconButton icon="arrow-left" label="Back to BraveBot" tooltip="Back to BraveBot" shortcut="⎋" size="tiny" onClick={back} />
            <span className="settings-crumb">Settings</span>
            <span className="settings-crumb-sep" aria-hidden="true">/</span>
            <span className="settings-crumb current">{current.label}</span>
          </div>
        </header>
        <div className="settings-scroll">
          <div className="settings-page">
            <h1 id="settings-title">{current.label}</h1>
            <p className="settings-subtitle">{current.subtitle}</p>
            {page === 'agent'
              ? <AgentSettings session={session} onChanged={onChanged} onDirty={onDirty} />
              : <GeneralSettings chosen={chosen} onAppearance={onAppearance} />}
          </div>
        </div>
      </main>
    </div>
  )
}
