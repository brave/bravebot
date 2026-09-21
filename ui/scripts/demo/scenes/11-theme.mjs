// The window, repainted.
//
// `View ▸ Theme…` is the one item in the menu bar that opens something a recording can show, so
// it follows the menu scene, which fires items the same way and has just explained why the bar
// itself cannot be filmed. The picker sits over the transcript rather than covering it, because
// what it previews is the window *behind* it: choosing a row repaints everything, and Escape puts
// back what was there. That round trip is the shot — a theme tried and not kept — so the take ends
// in the same dark the rest of the video is in.
//
// `brave` is the default and means no theme at all, which is why an exported PDF stays white
// however dark the window is; the caption says so because it is the thing about themes here that
// is not obvious from watching one apply.
import { openNewest } from '../pick.mjs'

/** Built-ins worth a look, in the order they are tried. Named so the take is the same each time. */
const TRY = ['nord', 'gruvbox', 'solarized']

export default {
  id: '11-theme',
  title: 'Themes',

  async run(s) {
    const { page, app } = s
    await openNewest(s, { hold: 1.0 })

    const opened = await app.evaluate(({ Menu }) => {
      const item = Menu.getApplicationMenu()?.getMenuItemById('view.theme')
      if (!item) return false
      item.click()
      return true
    })
    if (!opened) s.skip('this build has no View ▸ Theme… item')

    const picker = page.locator('.theme-picker')
    try {
      await picker.waitFor({ state: 'visible', timeout: 2000 })
    } catch {
      s.skip('the theme picker did not open')
    }

    await s.say('View ▸ Theme…', 'A picker over the transcript, because what it previews is the window behind it.', 2.6)
    await s.shot('11-theme')

    const rows = page.locator('.theme-row')
    const names = (await rows.locator('.theme-name').allInnerTexts()).map((n) => n.trim())
    await s.say('Built in', `${names.length} in the list, brave among them — and any palette written as JSON beside the app’s own state.`, 2.6)

    for (const wanted of TRY) {
      const at = names.findIndex((n) => n.toLowerCase().startsWith(wanted))
      if (at < 0) continue
      const row = rows.nth(at)
      await s.glideTo(row)
      await s.click(row)
      await s.say(names[at], 'Chosen, and the whole window follows — before anything is written down.', 2.2)
    }
    await s.shot('11-theme-preview')

    await s.say('brave', 'The default is no theme at all: the macOS palette, following the system. An exported PDF stays white.', 3.0)

    await s.say('Escape', 'Puts back what was there. Enter would have kept it.', 2.0)
    await page.keyboard.press('Escape')
    await s.beat(1.2)
  },
}
