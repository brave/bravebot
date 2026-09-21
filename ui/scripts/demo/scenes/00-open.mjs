// The title card, and the one fact about the app that the rest of the video assumes: it is a
// window onto the same sessions the terminal client uses, driven by an agent running as a
// child process rather than by a terminal being scraped.
export default {
  id: '00-open',
  title: 'Opening',

  async run(s) {
    const { page } = s
    await s.beat(1)
    await s.say('Brave Bot', 'A macOS interface to bravebot — the prompt-injection-resistant coding agent.', 3)
    await s.say('Brave Bot', 'The same sessions as the terminal client. A session begun here resumes with --resume.', 3)

    const build = page.locator('.build')
    if (await build.count()) {
      await s.spotlight(build, 1.2)
      await s.say('The agent', (await build.textContent()) ?? '', 2)
      await s.unspot()
    }
    await s.shot('00-open')
  },
}
