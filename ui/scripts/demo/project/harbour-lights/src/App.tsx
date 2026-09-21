import { useState } from 'react'
import { StationPicker } from './components/StationPicker'
import { TideChart } from './components/TideChart'
import stations from './data/stations.json'

/**
 * The whole app: a picker above a chart.
 *
 * The station lives here rather than in the picker because the chart needs it too, and a
 * picker that owned it would have to hand it back up through a callback to be useful.
 */
export function App(): React.JSX.Element {
  const [station, setStation] = useState(stations[0]!.id)
  const chosen = stations.find((s) => s.id === station) ?? stations[0]!

  return (
    <main className="app">
      <header>
        <h1>Harbour Lights</h1>
        <p className="where">{chosen.name}</p>
      </header>
      <StationPicker stations={stations} chosen={station} onChoose={setStation} />
      <TideChart station={chosen} hours={12} />
    </main>
  )
}
