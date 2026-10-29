# Playground

The playground runs the real renderer in a browser with a scripted bridge in place of the Rust
backend. It shows every piece the conversation area can render, so a change to one can be looked at
without a model, a key or an Electron window.

```bash
pnpm run playground
```

This serves `http://localhost:5199`. It opens the conversation "Everything the conversation can
show". Pass `?welcome` to start on the welcome screen.

## What it is

- `playground/bridge.ts` implements `BravebotApi` and installs it as `window.bravebot`. Requests
  are answered from `playground/fixtures.ts`. Events are emitted by the scenarios in
  `playground/scenarios.ts`.
- Nothing in `src/` is changed or imported conditionally. The renderer cannot tell the bridge from
  the real one.
- Stored histories cover every kind of stored message. Scenarios cover everything that is only
  produced live: decision cards, the working row, failures and plan runs.

## Using it

The Playground button at the bottom right opens a panel that lists the scenarios in groups. Each
scenario plays in the conversation that is open. The panel also has these controls.

- **Theme** follows the system, or is forced light or dark.
- **Speed** scales the pauses between events.
- **Answer questions for me** presses the buttons on the cards as they appear, so decided cards
  can be seen. It scrolls the card and any evidence inside it so each deciding row passes through
  the viewport, then presses. A card cannot be approved before those rows have been on screen.
  A card it cannot answer within 15 seconds ends the scenario with that failure.
- **Backend not set up** shows the screens a person sees before a model is configured. The choice is kept, and a reload shows it.

Typing `play:<id>` as a prompt plays that scenario. Other prompts get a short reply.

The console has the same controls:

```js
playground.play('tour')
playground.stop()
playground.scroll('top') // or 'end', or a number
playground.scenarios     // every id
playground.settings.auto = true
```

## Limits

- Native context menus, the file picker and the menu bar commands do not exist in a browser.
- Nothing is read from or written to disk. Sessions, bots and settings live in memory and are lost
  on reload. Theme, layout, the panel state and whether a backend is configured are kept in `localStorage`.
- The scenarios are the ones this repository knows about. A new event needs a fixture and a
  scenario added by hand.
