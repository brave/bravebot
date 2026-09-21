// The part of the product everything else is furniture around: a real turn, and the questions
// it stops to ask.
//
// Gated behind `--live`, because it calls the model, so it costs tokens and takes as long as it
// takes — and it is the only scene besides the bots' whose timing is not the demo's to decide: a
// turn can stall, or answer without needing to ask anything at all. It was last in the running
// order for that reason, so a bad take cost the tail of the video rather than the whole of it;
// it is third now, so a viewer meets the thing the app is for before the furniture around it
// (`manifest.mjs` says why), and a bad take is a retake with `--from`.
//
// The shape of the scene is a loop rather than a script, because a turn asks as many questions
// as it needs and the demo does not get to decide how many. Approving a command is not the end
// of it: the agent then has to ask whether it may *read what the command printed*, which is a
// second of the five and the one that most surprises people. An earlier version of this scene
// waited for the turn to go quiet after the first approval and hung there forever, waiting for
// a turn that was waiting for it.
//
// A write is never approved. The prompts ask for nothing to be written, but a model is not a
// promise, and a turn that decides to write anyway must not find the demo clicking yes on its
// behalf — a driver that approves whatever it is shown is the exact habit this app exists to
// argue against. So writes and vouches are declined, which is a legitimate answer, keeps the
// checkout as it was, and is a better shot than an approval nobody meant to give.

import { join } from 'node:path'

/** How each of the five reads on screen, in the order somebody meets them. */
const QUESTIONS = {
  run: {
    title: 'It wants to run a command',
    line: 'And stops, because that is not its decision to make.',
    answer: 'approve',
  },
  output: {
    title: 'May it read what that printed?',
    line: 'A separate question. Running a command and letting the model see the output are not the same permission.',
    answer: 'approve',
  },
  confirm: {
    title: 'It wants to write a file',
    line: 'Shown as a diff, in full, before anything reaches the disk.',
    answer: 'reject',
  },
  vouch: {
    title: 'It wants to vouch for a path',
    line: 'A standing rule rather than a one-off, which is why it is asked separately.',
    answer: 'reject',
  },
  ask: {
    title: 'It has questions for you',
    line: 'Put together rather than one at a time.',
    answer: 'approve',
  },
}

/**
 * Which of the five a card is, from the class the transcript already puts on it.
 *
 * Every card carries `confirm`, and four of the five carry a second word saying which — `run`,
 * `output`, `vouch`, `ask`. A write is the one with nothing else, so `confirm` is the fallback
 * and never a match to test for. Reading it the other way round — asking whether the class list
 * contains `confirm` before asking whether it contains `ask` — matches *every* card as a write,
 * which is how this scene came to decline two rounds of questions it was supposed to answer.
 */
const NAMED = ['run', 'output', 'vouch', 'ask']

const kindOf = async (card) => {
  const classes = ((await card.getAttribute('class')) ?? '').split(/\s+/)
  return NAMED.find((kind) => classes.includes(kind)) ?? 'confirm'
}

/**
 * Wait for whatever comes next: another question, or the end of the turn.
 *
 * Returns the card still asking, or `null` once the turn has gone quiet. A card that has been
 * answered stays in the transcript as the record of the decision, so `:has(.confirm-actions)`
 * is what separates a question from a receipt.
 */
async function nextQuestion(s, seconds = 180) {
  const asking = s.page.locator('.confirm:has(.confirm-actions)').first()
  const working = s.page.locator('.working')
  const until = Date.now() + seconds * 1000

  while (Date.now() < until) {
    if (await asking.isVisible().catch(() => false)) return asking
    if (!(await working.isVisible().catch(() => false))) {
      // A turn pauses between steps, so a single quiet frame is not the end of one.
      await s.page.waitForTimeout(2000)
      if (
        !(await working.isVisible().catch(() => false)) &&
        !(await asking.isVisible().catch(() => false))
      ) {
        return null
      }
      continue
    }
    await s.page.waitForTimeout(700)
  }
  return null
}

/** Film every question the turn puts up, answer it, and carry on until the turn is done. */
async function answerEverything(s, { shot }) {
  let seen = 0
  for (;;) {
    const card = await nextQuestion(s)
    if (!card) return seen

    const kind = await kindOf(card)
    const { title, line, answer } = QUESTIONS[kind]
    await s.glideTo(card)
    await s.say(title, line, 2.6)

    if (kind === 'run') {
      await s.spotlight(card.locator('.stages').first(), 2)
      await s.unspot()
      await s.say('The argv, as it will run', 'And what each name resolved to on this machine.', 2.6)
      await s.say('Nothing can be sent past it', 'The composer says which question is waiting.', 2.2)
      await s.say('Three answers', "Don't run · Run · Run and don't ask again.", 2.4)
    }
    if (kind === 'output') {
      // The bytes in full, never a preview: the person deciding whether the model may read
      // this has to be reading it themselves.
      await s.spotlight(card.locator('.preview').first(), 2.2)
      await s.unspot()
      await s.say('In full, not a preview', 'Approving is what puts it in the model’s context.', 2.6)
    }
    if (answer === 'reject') {
      await s.spotlight(card.locator('.diff, .preview, .path').first(), 2)
      await s.unspot()
      await s.say('Declined', 'Which is a real answer: the turn is told no and carries on.', 2.4)
    }
    if (kind === 'ask') {
      const blocks = await card.locator('.ask-question').all()
      for (const [n, block] of blocks.entries()) {
        const choices = block.locator('.choices .choice')
        const offered = await choices.count()

        if (offered > 0) {
          // Read the options before picking one, and pick a different one each time rather
          // than always the first — a demo that only ever takes the top option looks like a
          // default being accepted, which is the one thing this card is not.
          await s.spotlight(block.locator('.choices'), 1.6)
          await s.unspot()
          const pick = choices.nth(n % offered)
          const label = ((await pick.locator('.label').textContent()) ?? '').trim()
          await s.click(pick)
          // The choice says so itself once taken; if it did not, the click missed.
          if (!(await pick.evaluate((el) => el.classList.contains('picked')))) await pick.click()
          await s.say('Pick one', label || `one of ${offered}`, 1.8)
        } else {
          // Only when the question offers none. A question can ask for your own words, and
          // that is a different control rather than a fallback.
          await s.say('Or your own words', 'A question can ask for those instead.', 1.6)
          await s.slowType(block.locator('.typed').first(), 'whichever reads best at the call site')
        }
      }
      await s.say('Answered here', 'In the transcript, where the question was — and nowhere else.', 2.4)
    }

    await s.shot(`${shot}-${seen}-${kind}`)
    if (answer === 'reject') {
      await s.click(card.locator('.confirm-actions .reject').first())
    } else {
      // Never `.always`: running once and vouching for the program forever are different
      // answers, and a demo should not be seen giving the broader one by reflex.
      const once = card.locator('.confirm-actions .approve:not(.always)').first()
      await s.click((await once.count()) ? once : card.locator('.confirm-actions .approve').first())
    }
    seen++
    await s.beat(1)
  }
}

export default {
  id: '02-live',
  title: 'A live turn',
  live: true,

  async run(s) {
    const { page } = s

    // A session of its own, every take.
    //
    // This scene is the only one that *says* anything, and an earlier version said it into
    // whichever session happened to be newest — which meant every run left two more turns in
    // one of the world's seeded sessions. After a dozen takes that session was a wall of
    // development exhaust, and it is the first row in the list, so it was the wall the video
    // opened on. A new session per take costs nothing, keeps the seeded ones as they were
    // built, and is the better shot anyway: a conversation is watched starting from nothing.
    if (s.opts.world) {
      // The picker is a native sheet and would hang the take, so it answers itself. The path
      // is one the demo world laid out, not one the window composed.
      const project = join(s.opts.world, 'projects', 'harbour-lights')
      await s.app.evaluate(({ dialog }, where) => {
        dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [where] })
      }, project)

      await s.say('A new session', 'Against a checkout, which it asks about before it works there.')
      await s.click(page.locator('.new').first())
      await page.waitForTimeout(1500 * s.speed)
    } else {
      // `--real`: no world was laid out, so there is no checkout of ours to open one against.
      if ((await page.locator('.session').count()) === 0) s.skip('no sessions to open')
      await s.click(page.locator('.session').first())
      await page.waitForTimeout(1500 * s.speed)
    }

    if (await page.locator('.trust').isVisible().catch(() => false)) {
      await s.say('Trust the directory', 'Asked once, per checkout, before anything happens in it.', 2.2)
      await s.shot('02-trust')
      await s.click('.trust-actions .approve')
      await page.waitForTimeout(600 * s.speed)
    }

    // --- a series of questions, first --------------------------------------------------------
    //
    // Before anything else, and that ordering is load-bearing rather than a matter of taste.
    // A planner that has been shown untrusted content may not put questions to you at all —
    // that gate is upstream and deliberate, and `drive-ask.mjs` says the same — and the other
    // half of this scene approves *reading what a command printed*, which is exactly how a
    // conversation acquires some. Filmed the other way round, this card never appears.
    await s.say('Ask it something', 'A real turn, against a real model.')
    await s.slowType(
      '.composer textarea',
      // Verbatim from `drive-ask.mjs`, which this borrows rather than re-invents: it is the
      // wording already known to make this agent reach for the question instead of simulating
      // one in its reply, and the second call gives the loop above a repeat to prove itself on.
      'I want to add a --json flag to this project. Do not write anything. First, in ONE ask_user ' +
        'call, ask me two questions together: which approach, and what to name the flag. After I ' +
        'answer, make a SECOND ask_user call with one further question. Then just summarise my ' +
        'answers.',
    )
    await s.click('.composer .send')
    await s.say('Working', 'The turn runs until it needs something only you can give it.', 1.6)

    if ((await answerEverything(s, { shot: '02-ask' })) === 0) {
      console.log('   the planner asked nothing — no series to film')
    }
    await s.shot('02-ask-done')
    await s.say('The answers are in the record', 'Beside the questions they answer.', 2.4)

    // --- a command, and then permission to read what it printed ------------------------------
    if (!(await page.locator('.composer textarea:not([disabled])').isVisible().catch(() => false))) {
      s.skip('the first turn never finished, so there is no composer to use')
    }
    await s.say('The other direction', 'Before it acts, it asks.')
    await s.slowType(
      '.composer textarea',
      'Run `wc -l src/lib/tides.ts` here and tell me how many lines it printed.',
    )
    await s.click('.composer .send')
    await s.beat(1.4)

    const answered = await answerEverything(s, { shot: '02-run' })
    if (answered === 0) console.log('   the turn asked nothing — no card to film')
    await s.say('And the turn carries on', 'Every one of those was a decision it could not make itself.', 2.4)
    await s.shot('02-run-done')

  },
}
