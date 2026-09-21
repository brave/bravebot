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

export function crownMotion(kind: 'none' | 'bobble' | 'antenna' | 'tuft') {
  switch (kind) {
    case 'bobble': return { stiffness: 105, damping: 10, response: 0.20, limit: 0.32 }
    case 'antenna': return { stiffness: 180, damping: 18, response: 0.09, limit: 0.18 }
    default: return { stiffness: 220, damping: 26, response: 0.02, limit: 0.05 }
  }
}
