// The keys, and the one thing no key can do.
//
// AppKit draws the menu bar itself, and Playwright's keyboard reaches the web contents over
// CDP and no further — so neither the menu nor its accelerators can be opened or pressed from
// here. `drive-menu.mjs` documents the same limit. What this scene does instead is fire each
// command through the menu item that owns it, which is the identical code path, and caption
// the key that would have done it. The footage is honest: those keys really do run that.
//
// The beat this whole scene exists for is the last one. **No key answers a question.** The
// five things the agent can ask are answered in the transcript and nowhere else, and the
// absence is structural rather than an oversight — the dispatch table in
// `src/renderer/commands.ts` is simply never given the callbacks that answer.
import { openNewest } from '../pick.mjs'

const KEYS = [
  ['⌘N', 'New chat, in the project used last'],
  ['⇧⌘W', 'Close the chat, ⌘W still closes the window'],
  ['⌘↵', 'Send'],
  ['⌘.', 'Cancel the running turn'],
  ['⌥⌘← / ⌥⌘→', 'Fold the chat list / the context panel'],
  ['Esc', 'Cancel from the composer, or clear the chat search'],
]

export default {
  id: '10-commands',
  title: 'The menu, and the keys',

  async run(s) {
    const { page, app } = s
    await openNewest(s, { hold: 1.2 })

    const fire = (id) =>
      app.evaluate(({ Menu }, wanted) => {
        const item = Menu.getApplicationMenu()?.getMenuItemById(wanted)
        if (item) item.click()
      }, id)

    await s.say('The menu', 'Which is mostly where the keys are written down.', 2)

    for (const [key, what] of KEYS) await s.say(key, what, 1.7)

    await s.say('⌥⌘←', 'Folds the chat list.')
    await fire('view.fold-left')
    await s.beat(1.4)
    await s.say('⌥⌘→', 'And the context panel.')
    await fire('view.fold-right')
    await s.beat(1.6)
    await s.shot('10-folded')
    await fire('view.fold-right')
    await s.beat(0.8)
    await fire('view.fold-left')
    await s.beat(1)

    // The two ways to start one, which sit beside the chat search: a new project, and a new chat
    // in the project used last.
    const newProject = page.locator('[data-test="new-project"]')
    if (await newProject.count()) {
      await s.say('New project…', 'Opens a folder; the plus beside it starts a chat in the project used last.')
      await s.pointAt(newProject)
      await s.beat(1)
      await s.pointAt('[data-test="new-session"]')
      await s.beat(1)
    }

    await s.say(
      'No key answers a question',
      'A write, a command, reading its output, a vouch, a series of questions — all answered in the transcript.',
      3.2,
    )
    await s.say(
      'No key answers a question',
      'An approval is a claim that somebody looked. A keystroke can be muscle memory into a window that changed a frame ago.',
      3.4,
    )
  },
}
