import { useId, type ReactNode } from 'react'

/** Whether a section title answers the settings search. Every term has to appear in it. */
export function matchesQuery(title: string, query: string): boolean {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean)
  return terms.every((term) => title.toLowerCase().includes(term))
}

/** One section of a settings page: a heading over a card. */
export function SettingsGroup({ title, note, children }: {
  title: string
  /** A short state beside the heading, such as unsaved changes. */
  note?: string
  children: ReactNode
}): React.JSX.Element {
  const id = useId()
  return (
    <section className="settings-group" aria-labelledby={id}>
      <h2 id={id} className="settings-group-title">
        {title}
        {note && <span className="settings-group-note">{note}</span>}
      </h2>
      <div className="settings-card">{children}</div>
    </section>
  )
}
