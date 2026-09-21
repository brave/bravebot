export type Row = { id: string; name: string; m2: number; s2: number; k1: number; o1: number }

/**
 * Read the export, whatever order its columns arrived in.
 *
 * Forgiving about the header and strict about the body: a column that moved is somebody
 * else's spreadsheet habit, but a constituent that will not parse is a number nobody should
 * be predicting a tide from, so it throws rather than defaulting to zero.
 */
export function parseTable(csv: string): Row[] {
  const [header, ...lines] = csv.trim().split(/\r?\n/)
  if (!header) return []

  const columns = header.split(',').map((name) => name.trim().toLowerCase())
  const at = (name: string): number => {
    const index = columns.indexOf(name)
    if (index < 0) throw new Error(`the export has no "${name}" column`)
    return index
  }

  const places = { id: at('id'), name: at('name'), m2: at('m2'), s2: at('s2'), k1: at('k1'), o1: at('o1') }

  return lines.filter(Boolean).map((line, n) => {
    const cells = line.split(',')
    const number = (key: 'm2' | 's2' | 'k1' | 'o1'): number => {
      const value = Number(cells[places[key]])
      if (!Number.isFinite(value)) throw new Error(`row ${n + 2}: ${key} is not a number`)
      return value
    }
    return {
      id: (cells[places.id] ?? '').trim(),
      name: (cells[places.name] ?? '').trim(),
      m2: number('m2'),
      s2: number('s2'),
      k1: number('k1'),
      o1: number('o1'),
    }
  })
}
