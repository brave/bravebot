type Station = { id: string; name: string }

/** A plain list rather than a <select>, so the whole set is readable at a glance on a phone. */
export function StationPicker({
  stations,
  chosen,
  onChoose,
}: {
  stations: readonly Station[]
  chosen: string
  onChoose: (id: string) => void
}): React.JSX.Element {
  return (
    <nav className="stations" aria-label="Which station">
      {stations.map((station) => (
        <button
          key={station.id}
          className="station"
          aria-pressed={station.id === chosen}
          onClick={() => onChoose(station.id)}
        >
          {station.name}
        </button>
      ))}
    </nav>
  )
}
