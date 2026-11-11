/** Deterministic timing keeps two views of the same bot moving together. */
export function randomUnit(seed: string): number {
  let value = 0x811c9dc5
  for (let at = 0; at < seed.length; at++) {
    value ^= seed.charCodeAt(at)
    value = Math.imul(value, 0x01000193)
  }
  // Avalanche neighbouring event numbers as well as neighbouring bot seeds.
  value ^= value >>> 16
  value = Math.imul(value, 0x7feb352d)
  value ^= value >>> 15
  return (value >>> 0) / 0x100000000
}

export function smooth(x: number): number {
  const c = Math.min(1, Math.max(0, x))
  return c * c * (3 - 2 * c)
}

/** A short glance surrounded by long, genuinely still pauses. */
export function glance(seconds: number, seed: string): number {
  const cycle = Math.floor(seconds / 12)
  const start = 5 + randomUnit(`${seed}/glance/${cycle}`) * 2
  const at = seconds % 12 - start
  const direction = randomUnit(`${seed}/direction/${cycle}`) < 0.5 ? -1 : 1
  return direction * smooth(at / 0.7) * (1 - smooth((at - 1.2) / 1.1))
}

/** Irregular blinks, occasionally doubled. Independent of state changes, so a lid never jumps. */
export function blink(seconds: number, seed: string): number {
  const cycle = Math.floor(seconds / 7)
  const start = 1 + randomUnit(`${seed}/blink/${cycle}`) * 3.5
  const at = seconds % 7 - start
  const one = (elapsed: number): number => {
    if (elapsed < 0 || elapsed >= 0.16) return 1
    return elapsed < 0.05
      ? 1 - smooth(elapsed / 0.05) * 0.92
      : 0.08 + smooth((elapsed - 0.05) / 0.11) * 0.92
  }
  const double = randomUnit(`${seed}/double/${cycle}`) < 0.18
  return Math.min(one(at), double ? one(at - 0.24) : 1)
}

/** Finish returning to eye contact, pause, then acknowledge completion with one nod. */
export function completionNod(elapsed: number): number {
  const at = (elapsed - 0.55) / 0.5
  if (at <= 0 || at >= 1) return 0
  return Math.sin(Math.PI * at) ** 2 * 0.22
}

/** How often a face at rest may fidget, in seconds: once in each window of this length. */
export const FIDGET_CYCLE = 32

/**
 * When in its current window a face at rest fidgets, and how. The moment falls between 8 and 24
 * seconds into the window, so two fidgets are at least 16 seconds apart, and it depends only on
 * the seed and the window, so two views of one bot fidget together.
 */
export function fidget(seconds: number, seed: string): { cycle: number; start: number; kind: number } {
  const cycle = Math.floor(seconds / FIDGET_CYCLE)
  return {
    cycle,
    start: cycle * FIDGET_CYCLE + 8 + randomUnit(`${seed}/fidget/${cycle}`) * 16,
    kind: randomUnit(`${seed}/fidget-kind/${cycle}`),
  }
}
