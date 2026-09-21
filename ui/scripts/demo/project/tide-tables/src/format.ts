import type { Row } from './parse.ts'

/** The shape `harbour-lights` reads: constants in the order its `PERIODS` are declared in. */
export function asStations(rows: readonly Row[]): unknown[] {
  return rows.map((row) => ({
    id: row.id,
    name: row.name,
    constants: [row.m2, row.s2, row.k1, row.o1],
  }))
}
