# harbour-lights

A small dashboard for tide predictions at a handful of coastal stations. Pick a station,
see the next twelve hours of predicted height, and a plain reading of high and low water.

Predictions come from a static table under `src/data`. Nothing is fetched at runtime, which
is deliberate: the whole point of the thing is to work on a boat with no signal.

```bash
npm install
npm run dev
```

## Layout

| Path | What |
| --- | --- |
| `src/App.tsx` | The one screen: a picker above a chart |
| `src/components/` | The picker and the chart |
| `src/lib/tides.ts` | Height at a time, and the turning points either side of it |
| `src/data/stations.json` | The stations, and their harmonic constants |
