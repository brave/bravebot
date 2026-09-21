// Writing a conversation to a file.
//
// The interesting thing about export is what it leaves out. By default the file carries the
// exchange — what was asked and what came back — and never the diffs, the approval cards or
// the confined blobs, because those are evidence laid out to be read in place and a document
// made out of one reads like a record of the exchange without being one. **Include Tool
// Calls** moves exactly one of those things across the line, and either way the file ends
// with a footer saying what it actually carried.
//
// The native save panel is a modal sheet that would hang the take behind something nobody can
// dismiss, so the stage has already pointed `showSaveDialog` at a temporary directory. The
// PDF is real: it is drawn by a second renderer using the same React components the window
// does, so a reply's markdown is gated on the way to paper by exactly what gates it on screen.
import { readFileSync } from 'node:fs'
import { findSession, openSession, openNewest } from '../pick.mjs'

export default {
  id: '09-export',
  title: 'Export',

  async run(s) {
    const { page, app } = s

    const busy = await findSession(s, (said) => said.some((line) => line.kind === 'tool'))
    if (busy) await openSession(s, busy, { hold: 1.2 })
    else await openNewest(s, { hold: 1.2 })

    const trigger = page.locator('.export-open')
    if ((await trigger.getAttribute('disabled')) !== null) s.skip('nothing has been said in this session yet')

    // The transcript scrolls itself to the bottom when a session opens, and `PopMenu` closes
    // on any scroll — so the menu goes up only once the column has stopped moving.
    await s.beat(2)
    await s.say('Export', 'Beside Send — and in the File menu, which offers the same three.')
    if (!(await s.openMenu(trigger))) s.skip('the export menu would not stay open')
    await s.say('Three formats', 'Plain text, Markdown, or a PDF that keeps the window’s own bubbles.', 2.4)
    await s.shot('09-menu')

    await s.say(
      'What it leaves out',
      'By default: the conversation. Never the diffs, the approval cards or the confined blobs.',
      3,
    )

    await s.click(page.locator('[role="menuitem"]').filter({ hasText: 'Markdown' }).first())
    await page.waitForTimeout(1400 * s.speed)

    // A saved file reports itself, and the modal has to come down before anything else can be
    // clicked — the scrim under it swallows every pointer event in the window.
    const saved = await s.dismissNotice({ hold: 0 })
    if (saved) await s.say('Saved', saved.replace(/\s+/g, ' ').slice(0, 110), 2.2)
    await s.dismissNotice({ hold: 0 })

    const written = async () => (await app.evaluate(() => globalThis.__exported)).at(-1)
    const mdPath = await written()
    if (mdPath) {
      const tail = readFileSync(mdPath, 'utf8').trim().split('\n').at(-1) ?? ''
      await s.say('And it says so', tail.replace(/^[*_\s>]+|[*_\s]+$/g, '').slice(0, 120), 3)
    }

    // The setting, and the same export again — so the difference is the shot rather than a
    // claim about one.
    await s.say('Include Tool Calls', 'Adds the steps between: the same verb, target and outcome, and nothing more.')
    await s.openMenu(trigger)
    const checkbox = page.locator('[role="menuitemcheckbox"]').first()
    await s.click(checkbox)
    await s.beat(0.8)

    await s.openMenu(trigger)
    await s.click(page.locator('[role="menuitem"]').filter({ hasText: 'Markdown' }).first())
    await page.waitForTimeout(1400 * s.speed)
    await s.dismissNotice({ hold: 1 })
    const withTools = await written()
    if (withTools) {
      const tail = readFileSync(withTools, 'utf8').trim().split('\n').at(-1) ?? ''
      await s.say('The footer changes with it', tail.replace(/^[*_\s>]+|[*_\s]+$/g, '').slice(0, 120), 3)
    }

    // Off again: it is not remembered across launches, and leaving it on would misrepresent
    // the default to whoever watches the next take.
    await s.openMenu(trigger)
    await s.click(checkbox)
    await s.beat(0.6)

    await s.say('PDF', 'Drawn by the same components the window uses, so the markdown is gated the same way.')
    await s.openMenu(trigger)
    await s.click(page.locator('[role="menuitem"]').filter({ hasText: 'PDF' }).first())
    await page.waitForTimeout(2600 * s.speed)
    await s.shot('09-export')
    await s.say('PDF', `Written to ${(await written()) ?? s.exports}`, 2.4)
    await s.dismissNotice({ hold: 1 })
  },
}
