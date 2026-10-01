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

    const build = await page.locator('.sessions').getAttribute('data-build').catch(() => null)
    if (build) await s.say('The agent', build, 2)
    await s.shot('00-open')
  },
}
