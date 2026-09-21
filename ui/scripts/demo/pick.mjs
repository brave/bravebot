// Choosing what to film.
//
// The scenes want different material — one with a run of tool calls in it, one with two
// prompts to cut between, one with a reply worth showing rendered — and which stored session
// has which is not knowable from the list, whose rows carry only a title and a checkout. So
// a scene says what it needs and this asks the agent, session by session, until something
// fits. `drive-fork.mjs` opens and closes sessions the same way to find one with two prompts.
//
// `--session` short-circuits all of it. Recording a second take of a video that came out well
// means filming the *same* session again, and the honest way to guarantee that is to name it
// rather than to hope the search lands on it twice.

/** Every stored session, newest first, as the list shows them. */
export const listed = async (s) => (await s.request('session.list', {})).ok?.sessions ?? []

/**
 * The newest stored session whose contents satisfy `fits`, which is handed the agent's own
 * `said` lines. Opens each candidate to look inside and closes it again, so nothing is left
 * running behind the scene that eventually gets filmed.
 */
export async function findSession(s, fits, { limit = 12 } = {}) {
  const all = await listed(s)
  const named = s.opts.session
    ? all.filter((x) => x.title?.toLowerCase().includes(s.opts.session.toLowerCase()))
    : all

  for (const session of named.slice(0, limit)) {
    const opened = await s.request('session.open', { directory: session.directory, id: session.id })
    const said = opened.ok?.said ?? []
    await s.request('session.close', { session: opened.ok?.session })
    if (fits(said, session)) return session
  }
  return null
}

/** Open a session by clicking its row, which is what the video is meant to show happening. */
export async function openSession(s, session, { hold = 1.6 } = {}) {
  const row = s.page.locator('.session').filter({ hasText: session.title }).first()
  if (!(await row.count())) s.skip(`"${session.title}" is not in the list`)
  await s.click(row)
  await s.page.waitForTimeout(1400 * s.speed)
  await s.beat(hold - 1)
  return row
}

/** The newest session, whatever is in it — enough for the scenes that only need a transcript. */
export async function openNewest(s, opts) {
  const all = await listed(s)
  const wanted = s.opts.session
    ? all.find((x) => x.title?.toLowerCase().includes(s.opts.session.toLowerCase()))
    : all[0]
  if (!wanted) s.skip(s.opts.session ? `no session matches --session ${s.opts.session}` : 'no sessions are stored')
  await openSession(s, wanted, opts)
  return wanted
}
