import { heightAt, turningPoints } from '../lib/tides'

type Station = { id: string; name: string; constants: number[] }

/**
 * Twelve hours of predicted height, drawn as an inline SVG.
 *
 * Inline rather than a charting library: this is one polyline over a fixed viewBox, and a
 * dependency for that would be most of the bundle.
 */
export function TideChart({
  station,
  hours,
}: {
  station: Station
  hours: number
}): React.JSX.Element {
  const now = Date.now()
  const steps = hours * 4 // a point every fifteen minutes
  const points = Array.from({ length: steps }, (_, i) => {
    const at = now + i * 15 * 60 * 1000
    return `${(i / (steps - 1)) * 100},${50 - heightAt(station.constants, at) * 8}`
  })

  const [low, high] = turningPoints(station.constants, now, hours)

  return (
    <figure className="chart">
      <svg viewBox="0 0 100 60" role="img" aria-label={`Tide at ${station.name}`}>
        <polyline points={points.join(' ')} fill="none" strokeWidth="0.8" />
      </svg>
      <figcaption>
        Low water {low} · High water {high}
      </figcaption>
    </figure>
  )
}
