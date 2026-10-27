/**
 * A label as a screen draws it, read only from the structured fields the agent sends: the trail's
 * short form (`(T,pub)`) or an audit record's `{integrity, confidentiality}` words.
 *
 * Exact tokens only. Anything else reads as untrusted and private, the pessimistic corner of the
 * lattice (protocol.ts rule 2), so a label nobody can read is never drawn as better than it was.
 */
export interface ShownLabel {
  trusted: boolean
  public: boolean
}

const SHORT: Record<string, ShownLabel> = {
  '(T,pub)': { trusted: true, public: true },
  '(T,priv)': { trusted: true, public: false },
  '(U,pub)': { trusted: false, public: true },
  '(U,priv)': { trusted: false, public: false },
}

const PESSIMISTIC: ShownLabel = { trusted: false, public: false }

export function parseLabel(label: unknown): ShownLabel {
  if (typeof label === 'string') return Object.hasOwn(SHORT, label) ? SHORT[label]! : PESSIMISTIC
  if (!label || typeof label !== 'object' || Array.isArray(label)) return PESSIMISTIC
  const { integrity, confidentiality } = label as { integrity?: unknown; confidentiality?: unknown }
  return { trusted: integrity === 'trusted', public: confidentiality === 'public' }
}

/** One fixed sentence per axis value. None carries anything the content said. */
export const LABEL_TOOLTIPS = {
  trusted: 'Trusted: it came from you, the agent, or a path you vouched for.',
  untrusted: 'Untrusted: nobody vouched for where it came from, so it cannot decide what happens.',
  public: 'Public: nothing private went into it.',
  private: 'Private: it holds content from this machine, so it goes only where private content may.',
} as const

export function labelWords(label: ShownLabel): { integrity: 'Trusted' | 'Untrusted'; confidentiality: 'Public' | 'Private' } {
  return { integrity: label.trusted ? 'Trusted' : 'Untrusted', confidentiality: label.public ? 'Public' : 'Private' }
}
