import type { CrashReportsReason } from '../shared/protocol'
import { t } from './i18n'

/** The one line saying what keeps crash reports off, or `null` where the switch decides. */
export function crashReportsLockedLine(reason: CrashReportsReason | null): string | null {
  if (!reason) return null
  switch (reason.code) {
    case 'environment': return t.uiSettingsCrashReportsLockedEnvironment({ variable: reason.variable ?? '' })
    case 'admin': return t.uiSettingsCrashReportsLockedAdmin()
    case 'incognito': return t.uiSettingsCrashReportsLockedIncognito()
  }
}
