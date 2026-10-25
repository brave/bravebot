// What the bot scenes share: the name of the bot `12-bots` makes, and how the later scenes find it.
//
// `13-bot-memory` and `14-bot-remembering` work on the bot that `12-bots` makes, so the name is
// held once here. This file is outside `scenes/` because `demo.mjs` refuses a file in that
// directory that the manifest does not list.
import { SIDEBAR_TAB } from '../drive-support.mjs'

/// A pattern matching exactly this name, with every metacharacter in it taken literally.
///
/// A bot name is display text rather than a pattern, so a name holding a `.` or a bracket would
/// otherwise match a row it does not name.
// nosemgrep: javascript.lang.security.audit.detect-non-literal-regexp.detect-non-literal-regexp
export const exactly = (name) => new RegExp(`^${name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}$`)

/** The bot `12-bots` makes on camera, and the scenes after it open. */
export const BOT_NAME = 'Harbour Watch'

/** The row in the bots list whose name is exactly `name`. */
export const botRow = (page, name) =>
  page.locator('.bot').filter({ has: page.locator('.bot-name', { hasText: exactly(name) }) })

/** Open the left column's bots tab, bowing out of the scene where the build has none. */
export async function openBotsTab(s) {
  const tab = s.page.locator(SIDEBAR_TAB).nth(1)
  if (!(await tab.count())) s.skip('this build has no bots tab')
  await s.click(tab)
  await s.page.waitForTimeout(600)
}
