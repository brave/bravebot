// The running order.
//
// This is the file to edit when a feature lands: add a scene under `scenes/` and add it here,
// in the place in the story it belongs. Nothing else needs touching — `demo.mjs` checks this
// list against the directory and refuses to run if the two disagree, so a scene written and
// never listed is an error rather than a scene that silently never plays.
//
// The order is a story rather than a feature list: find a session, watch a turn happen and be
// questioned, then read the record it left, arrange the window around it, and look at what the
// agent touched. Forking and export come next because they are things done to a conversation
// that already exists.
//
// The theme picker follows the menu, because it is the one item in the bar that opens something a
// recording can show, and the menu scene has just explained why the bar itself cannot be.
//
// The bots come last, after all of that, because they are the one thing here that is not about a
// session at all — a second kind of thing in the same window, and easier to follow once the first
// kind has been shown whole. They are three scenes for the reason `02-live` is its own: what a bot
// looks like costs nothing to film, and what a bot *does* needs a model to answer.
export const SCENES = [
  '00-open',
  '01-sessions',
  // `live: true` — filmed only with --live. Third, so a viewer meets the thing the app is for
  // before meeting the furniture around it: a real turn, and the questions it stops to ask.
  // It also leaves the session richer than it found it — approval cards in the transcript,
  // confined content in the context panel — which every scene after it then has to show.
  '02-live',
  '03-transcript',
  '04-columns',
  '05-context',
  '06-file-tree',
  '07-markdown',
  '08-fork',
  '09-export',
  '10-commands',
  '11-theme',
  '12-bots',
  // `live: true` — filmed only with --live, and only after `12-bots`, which makes the bot this
  // one opens.
  '13-bot-memory',
  // `live: true`, and after `13-bot-memory` because it reads the session that one leaves behind.
  // It is about the memory a bot keeps when nobody has asked it to, so most of what it can show
  // depends on what the take before it earned — and it bows out rather than films an empty frame.
  '14-bot-remembering',
  '99-close',
]
