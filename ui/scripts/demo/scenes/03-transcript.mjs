// The middle column: the conversation itself.
//
// Two things here are the product rather than the furniture, and both get their own beat. A
// turn's tool calls are gathered into a *run* that folds away, so a transcript reads as an
// exchange rather than as a log. And anything the agent was shown from outside is drawn as
// confined content — labelled as what it is, not as text the model simply read — which is the
// whole argument the app is making.
import { findSession, openSession, openNewest } from '../pick.mjs'

export default {
  id: '03-transcript',
  title: 'The transcript',

  async run(s) {
    const { page } = s

    // A session with tool calls in it if there is one, since the fold is the shot.
    const busy = await findSession(s, (said) => said.some((line) => line.kind === 'tool'))
    if (busy) {
      await s.say('Open a session', `"${busy.title.slice(0, 60)}"`)
      await openSession(s, busy)
    } else {
      await s.say('Open a session', 'Click a row and the conversation fills in.')
      await openNewest(s)
    }

    await s.say('The conversation', 'What was asked, and what came back.', 1.8)
    await s.shot('03-transcript')

    const asked = page.locator('.bubble.user').first()
    if (await asked.count()) {
      await s.glideTo(asked)
      await s.spotlight(asked, 1.4)
      await s.unspot()
    }

    // The run of tool calls. A run of one is drawn as a plain line with no header, so this
    // looks for the header rather than for a tool row.
    const run = page.locator('.tool-run-head').first()
    if (await run.count()) {
      await s.glideTo(run)
      await s.say('Steps, gathered', "A turn's tool calls come as one run, not as a wall of lines.", 1.6)
      await s.click(run)
      await s.beat(1.2)
      await s.say('Steps, gathered', 'It folds away — and folds back.', 1)
      await s.click(run)
      await s.beat(1.2)
      await s.shot('03-tool-run')
    }

    // Confined content, if this session has any. Worth going to look for: it is the one part
    // of the transcript that exists because of the threat model rather than in spite of it.
    const confined = page.locator('.quarantine-head').first()
    if (await confined.count()) {
      await s.glideTo(confined)
      await s.say(
        'Confined content',
        'Anything from outside is shown as what it is — not as text the model just read.',
        2.4,
      )
      await s.spotlight(page.locator('.quarantine').first(), 1.6)
      await s.unspot()
      await s.shot('03-confined')
    }

    const reply = page.locator('.bubble.assistant').last()
    if (await reply.count()) await s.glideTo(reply)
  },
}
