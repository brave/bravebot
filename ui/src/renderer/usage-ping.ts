import type { UsagePingReason, UsagePingReport, UsagePingSaved } from '../shared/protocol'
import { t } from './i18n'

type Answer<T> = { ok?: T; error?: { code: string; message: string } }

/** The one line saying what keeps the switch off, or `null` where the switch decides. */
export function lockedLine(reason: UsagePingReason | null): string | null {
  if (!reason) return null
  switch (reason.code) {
    case 'environment': return t.uiSettingsUsagePingLockedEnvironment({ variable: reason.variable ?? '' })
    case 'setting': return t.uiSettingsUsagePingLockedSetting()
    case 'admin': return t.uiSettingsUsagePingLockedAdmin()
    case 'incognito': return t.uiSettingsUsagePingLockedIncognito()
  }
}

/** The report in an answer, or `null` where the answer is not one. */
export function reportIn(answer: Answer<UsagePingReport>): UsagePingReport | null {
  const ok = answer.ok
  if (!ok || typeof ok.enabled !== 'boolean' || typeof ok.changeable !== 'boolean') return null
  return { enabled: ok.enabled, changeable: ok.changeable, reason: ok.reason ?? null }
}

/**
 * Flip the switch and say what it shows afterwards. A refusal, or a file that was not written,
 * leaves the switch as it was, so it never shows a choice nothing kept.
 */
export async function changeUsagePing(
  request: (enabled: boolean) => Promise<Answer<UsagePingSaved>>,
  before: UsagePingReport,
  enabled: boolean,
): Promise<{ report: UsagePingReport; unsaved: boolean }> {
  try {
    const answer = await request(enabled)
    const after = reportIn(answer)
    if (after && answer.ok?.saved) return { report: after, unsaved: false }
    return { report: after ?? before, unsaved: true }
  } catch {
    return { report: before, unsaved: true }
  }
}
