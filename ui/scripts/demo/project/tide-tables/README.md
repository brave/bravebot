# tide-tables

Turns the Admiralty's published tide tables into the JSON `harbour-lights` reads. A
one-way conversion, run by hand when a new year's tables come out.

```bash
npm run build -- data/sample.csv > ../harbour-lights/src/data/stations.json
```

The CSV is whatever the tables were exported as, which changes slightly every year, so
`parse.ts` is deliberately forgiving about column order and strict about the numbers.
