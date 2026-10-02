import { useId, type ReactNode } from 'react'

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
