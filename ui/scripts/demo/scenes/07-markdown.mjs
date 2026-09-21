// A reply, rendered.
//
// Markdown is the one place model output stops being text and becomes DOM, so it is worth a
// beat of its own — and worth saying on screen what the renderer will not do with it. There
// is no `rehype-raw`, no `dangerouslySetInnerHTML`, an allow-list of link schemes and no
// images, which is why a reply can be formatted without a reply being able to reach anything.
import { findSession, openSession, openNewest } from '../pick.mjs'

/** Something in the reply that is unmistakably *rendered* rather than printed. */
const RICH = ['.bubble.assistant .md-table-wrap', '.bubble.assistant pre code', '.bubble.assistant h2', '.bubble.assistant ul']

export default {
  id: '07-markdown',
  title: 'Formatted replies',

  async run(s) {
    const { page } = s

    // The longest reply in the store is the likeliest to have been formatted.
    const wordy = await findSession(s, (said) =>
      said.some((line) => line.kind === 'assistant' && (line.text?.length ?? 0) > 600),
    )
    if (wordy) await openSession(s, wordy, { hold: 1.2 })
    else await openNewest(s, { hold: 1.2 })

    const reply = page.locator('.bubble.assistant').first()
    if (!(await reply.count())) s.skip('this session has no reply in it')

    await s.glideTo(reply)
    await s.say('Formatted replies', 'Headings, lists, tables and code, rather than a wall of text.', 2.2)
    await s.shot('07-markdown')

    for (const selector of RICH) {
      const found = page.locator(selector).first()
      if (!(await found.count())) continue
      await s.glideTo(found)
      await s.spotlight(found, 1.6)
      await s.unspot()
      break
    }

    await s.say(
      'And nothing more',
      'No raw HTML, no images, and a link has to be http, https or mailto to survive.',
      2.8,
    )

    const link = page.locator('.bubble a[href]').first()
    if (await link.count()) {
      await s.glideTo(link)
      await s.hover(link)
      await s.say('A link says where it goes', 'The text was written by the model; the destination is not.', 2.4)
    }
  },
}
