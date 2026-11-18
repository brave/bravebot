export function problemToShow(problem: string | null, lastShown: string | null, asked: boolean): string | null {
  if (problem === null) return null
  return asked || problem !== lastShown ? problem : null
}
