# Desktop prototype mode

Stage 5b of the [implementation plan](implementation-plan.md#5b-attach-the-desktop): the desktop application as a client of `bravebot-host`, so it and another client, such as the host test client or later a phone, work in one session. The behavior is specified in [HOSTCLIENT](../../specs/desktop-host-client.md).

## How it starts

The host starts the desktop as one of its clients, over the anonymous channel it makes for each client ([architecture](architecture.md#local-control-boundary)). The person names the client and the kinds it may answer on the host's command line, and passes the app its name and folder:

```sh
bravebot-host --sessions-root <root> --workspace <dir> \
  --client desktop --answers confirm,run,fetch,ask,trust --program <electron> \
  --arg <path to ui/> --arg --bravebot-host-client \
  --arg --host-client-name --arg desktop --arg --host-client-workspace --arg <dir>
```

The name is given twice, to `--client` and to `--host-client-name`, and the two must be the same: the host does not tell a client its name, and the window uses its own copy to say which action in the view is its own.

`pnpm run drive:prototype` in `ui/` starts exactly this, with the host test client as a second client, a model service it serves itself and a scratch home. The host does not start the desktop for the person yet. A desktop that attaches to a host that is already running is [later work](implementation-plan.md#end-goal-beyond-the-prototype), which the plan leaves out of 5b because the desktop and the host are launched together.

## The two approaches

The plan asks for a comparison before building: a main-process compatibility adapter that keeps the ordinary window and feeds it as if the host were its own agent, against the window consuming the shared Rust view directly.

**A main-process adapter under the ordinary window.** The ordinary window may call 60 bridge methods. The host offers a client 16, and 11 of those are among the 60, so 49 of the ordinary window's methods have no answer from the host (settings, model lists, saved sessions, bots, attachments, rewind, forks and the rest). The window builds its state by folding 44 kinds of raw event, and none of them carries the text of a prompt: `turn.started` names the turn, the mode and the sandbox, and the text is only in the view's rows. A second client's message would not reach the window unless the adapter made up an event from a row, and the window's own send puts the prompt in its transcript before the host has taken it, which is the copy of state that the plan's "remove renderer-local prompt insertion" removes. Every control that cannot work would have to be found and disabled one by one in a window of about 1,200 lines and its components, and a control missed would fail at its first use.

**The window consuming the view.** One window, drawn from the view and the host's own words about what this client may answer. The window's entries are made from the rows with the constructors the ordinary window uses for the same events, and its question cards are the ordinary cards, so a write, a command, a fetch and the planner's questions look and are marked as they do there. It offers what the host offers and nothing else, so there are no unsupported controls to disable. The cost is a second window and a set of new files (about 1,010 lines of source, 160 of style and 410 of unit tests, with a 260-line driver), and a card or an entry the ordinary window changes is changed here too, which is the point.

**Decision.** The second. The ordinary window, its preload, its agent process and its start-up are not changed. The one edit to its code is an optional note on a settled question in the transcript's row component. The build has a second entry that loads only the prototype window when the app is given the host-client flag. The main process holds the channel and the shared TypeScript client, `packages/agent-client`, the same client the mobile app and the host test client use, and it tells the window the client's state. Nothing in the window recomputes what the Rust view says.

Test implications. The main-process half is tested with a script that stands in for the host. The window's transcript is tested by rendering the ordinary cards from view rows. The whole is driven by `scripts/drive-prototype.mjs` against the real host and the real app, with the other client's side read from the host's own output, so the window is not the only witness to what it shows.

## Capability matrix

| Capability | Prototype window | Ordinary desktop |
|---|---|---|
| Open a session in the one folder it was given | yes | any project folder |
| Join a session another client opened | yes | no |
| List the host's sessions | yes, and whether this client is attached | the person's saved sessions |
| Send a message, as typed | yes. A send during a turn is refused as busy and the message stays in the box | yes, with a queue and interjection |
| Cancel the turn on screen | yes | yes |
| Answer a write, a command or a fetch | yes, where the host gave this client the kind | yes |
| Answer the planner's questions | yes, typed or chosen, or declined | yes |
| Answer startup trust | yes, where the host gave this client `trust`, and only while the view says the question is open. A yes here does not cover what a yes in the ordinary window also grants for confined commands, so the dialog does not offer it. Otherwise the window says that another client the host trusts with it answers it | yes |
| See which client acted last | yes | no |
| Close a session | no. Closing the window's view or the window closes nothing | yes |
| Other approvals (output, vouch, checker, language server, plan, MCP, hosts, exposure) | drawn as a line to answer on another device | yes |
| Attachments, `@` files, pasted images, dropped files | no | yes |
| Bots, forks, watches, manifests, rewind | no | yes |
| Settings, hooks, models, accounts, sign-in | no | yes |
| Saved sessions, import, export, search | no | yes |
| File tree and preview | no | yes |
| A turn that arrives while the window is closed | kept by the host and drawn from its view when the window opens it | not applicable |

## Limits

A session whose view ended from a gap in the updates or a protocol error cannot be opened again until the app restarts, because the shared client keeps the session object and refuses a second attach of it; a session another client closed can be opened again. A view keeps the most recent rows and counts the rest as omitted, so the window can show fewer rows than the session has. The window draws a question whose card its entries support, and any other kind as a line. It is for local use with the host's anonymous channel, and does not decide the networking questions of stages 6 and 7.
