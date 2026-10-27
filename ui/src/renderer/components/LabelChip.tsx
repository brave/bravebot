import { LABEL_TOOLTIPS, labelWords, parseLabel } from '../labels'
import { Label } from '../nala'

/**
 * A label's two axes, as two quiet chips. Neutral on purpose: a trusted label is not a reason to
 * relax, so nothing here is green, and a chip says what the label is, not that anything read it.
 */
export function LabelChip({ label }: { label: unknown }): React.JSX.Element {
  const shown = parseLabel(label)
  const words = labelWords(shown)
  return <span className="label-chip" role="group" aria-label={`${words.integrity}, ${words.confidentiality.toLowerCase()}`} data-test="label-chip">
    <Label mode="outline" color="neutral" className="label-axis" data-tooltip={shown.trusted ? LABEL_TOOLTIPS.trusted : LABEL_TOOLTIPS.untrusted}>{words.integrity}</Label>
    <Label mode="outline" color="neutral" className="label-axis" data-tooltip={shown.public ? LABEL_TOOLTIPS.public : LABEL_TOOLTIPS.private}>{words.confidentiality}</Label>
  </span>
}
