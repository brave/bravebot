/**
 * Height at a time, from a station's harmonic constants.
 *
 * Four constituents is enough for a dashboard and nowhere near enough for navigation, which
 * the README says out loud. The point here is a readable curve, not a prediction anybody
 * should sail on.
 */
const PERIODS = [12.4206, 12.0, 25.8194, 23.9345] // M2, S2, K1, O1, in hours

export function heightAt(constants: readonly number[], when: number): number {
  const hours = when / 3_600_000
  return constants.reduce((sum, amplitude, i) => {
    const period = PERIODS[i]
    if (period === undefined) return sum
    return sum + amplitude * Math.cos((2 * Math.PI * hours) / period)
  }, 0)
}

/** The next low and high water inside `hours`, as `HH:MM`, sampled rather than solved. */
export function turningPoints(
  constants: readonly number[],
  from: number,
  hours: number,
): [string, string] {
  let low = { at: from, height: Infinity }
  let high = { at: from, height: -Infinity }

  for (let minute = 0; minute <= hours * 60; minute += 5) {
    const at = from + minute * 60_000
    const height = heightAt(constants, at)
    if (height < low.height) low = { at, height }
    if (height > high.height) high = { at, height }
  }

  const clock = (at: number): string =>
    new Date(at).toTimeString().slice(0, 5)
  return [clock(low.at), clock(high.at)]
}
