/**
 * A label as a screen draws it, read only from the `{integrity, confidentiality}` words the bridge
 * names it by (`labelAxes`, and an audit record's `label`). The trail's short form is not read here:
 * the agent's `Label::named` is its one reader (SHARE-001).
 *
 * Exact words only. Anything else reads as untrusted and private, the pessimistic corner of the
 * lattice (protocol.ts rule 2), so a label nobody can read is never drawn as better than it was.
 */
export interface ShownLabel {
  trusted: boolean
  public: boolean
}

export function parseLabel(label: unknown): ShownLabel {
  if (!label || typeof label !== 'object' || Array.isArray(label)) return { trusted: false, public: false }
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
