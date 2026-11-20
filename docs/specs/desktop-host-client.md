---
id: HOSTCLIENT
title: The desktop as a client of the local host
status: normative
governs:
  - ui/src/main/entry.ts
  - ui/src/main/prototype/options.ts
  - ui/src/main/prototype/client.ts
  - ui/src/main/prototype/main.ts
  - ui/src/preload/prototype.ts
  - ui/src/shared/prototype.ts
  - ui/src/renderer/prototype.tsx
  - ui/src/renderer/prototype/entries.ts
  - ui/src/renderer/prototype/keyed.ts
  - ui/src/renderer/prototype/PrototypeApp.tsx
documented-by: none (internal: the desktop is started as a host client by a person from the host's command line; nothing in the product launches it that way yet)
---

## Scope

The desktop application started by `bravebot-host` as one of its clients ([HOST-1](local-host.md#HOST-1)), so it and another client attach to the same session ([HOST-7](local-host.md#HOST-7)). In that mode the app is one window drawn from the shared view of [RPCVIEW-1](session-view.md#RPCVIEW-1), and it offers what the host offers a client and nothing else. The ordinary desktop, with its own agent process, its sessions, bots, settings and files, is the app as it always was and is not changed by this.

Not here: the desktop attaching to a host that was already running when it started, which needs a decision of its own ([U4](../design/mobile/open-questions.md#u4-attaching-a-client-the-host-did-not-launch)), a session list of the person's saved sessions, attachments, bots, forks, watches, manifests, settings, file browsing, export and any other control the host does not offer ([the capability matrix](../design/mobile/desktop-prototype.md#capability-matrix)).

## Clauses

<a id="HOSTCLIENT-1"></a>
### HOSTCLIENT-1: the desktop is a host client only when the host started it as one, and then it is the prototype window and nothing else

The desktop is a host client when it is given `--bravebot-host-client`, with `--host-client-name` the name the person gave it on the host's command line and `--host-client-workspace` the absolute path of the folder it opens sessions in. An argument set that has the flag and is missing either, or names the client other than 1 to 32 of `a-z`, `0-9` and `-`, or gives a relative path, ends the process with a message on standard error. No environment variable makes it one. Started this way it opens the prototype window, and it starts no agent process, reads no saved session and runs none of the ordinary app's start-up work. The host's channel is its standard input and output, so nothing but the channel is written to standard output.

`verified-by: by-construction (the main process is not a crate this workspace compiles, so ui/scripts/prototype-client.test.mjs holds that the flag with a name and a folder makes the desktop a client, that neither alone does, and that a wrong name, a missing folder and a relative folder are refused; ui/scripts/drive-prototype.mjs starts the real host with the real app as a client and checks that it connects, lists no session of its own and shows the prototype window)`

<a id="HOSTCLIENT-2"></a>
### HOSTCLIENT-2: the window is drawn from the view the host sends, and a question is drawn only for what this client may answer

The transcript is made from the rows of the session's view, with the desktop's own entries for each row's event, and a status, a question waiting and the client that acted last are the view's. A message another client sent is drawn from the view alone, and a message this window sent is not drawn until the view has it, so there is no copy of a prompt kept in the window. A question waiting is a card with its buttons only where the kinds the host gave this client include its kind. Another kind is drawn as one to answer on another device, with no button. A question that is no longer waiting says it was settled, and says what this window answered only where this window answered it and the host took the answer, since the view does not say who answered or whether it was refused. A question in a view that has ended is not offered. A row whose payload cannot be drawn is a line saying so and does not stop the others.

`verified-by: by-construction (ui/scripts/prototype-window.test.mjs makes the entries from rows and holds that a prompt another client sent is an entry with a key of its own, that a question the client may answer has buttons and one it may not has none, that a settled question is not drawn as approved unless this window answered it, that a question in an ended view is not offered, and that a payload that is not an object and a kind the window cannot draw are lines; ui/scripts/drive-prototype.mjs shows the other client's message in the window with no send from it, and the client that acted last in each client's view)`

<a id="HOSTCLIENT-3"></a>
### HOSTCLIENT-3: the window may ask the main process for a short list of actions and nothing else

The window has no `window.bravebot`. Its preload gives it the state and one call, and the call takes an action from this list: refresh what the host holds, open a session in the folder, join a session the host holds, close the window's view of a session, send, cancel, answer an approval, answer the planner's questions and answer startup trust. The last five name the session they are for, and one for a session that is not the one on screen is refused and sent nowhere, because question numbers are each session's own. When two sessions are chosen one after another, the later one stays on screen whatever order the host answers in. The main process takes an action only from its own window's main frame, and refuses a value of another shape, a field the action does not have, an empty or very long message and a request number that is not a count. An action reaches the host as the request the shared client makes for it, so the host's own limits on a client ([HOST-3](local-host.md#HOST-3)) apply to it and a refusal comes back with the host's code.

`verified-by: by-construction (ui/scripts/prototype-client.test.mjs holds the actions the window may ask for and a set of values it may not, that an action with no session open reaches the host as nothing, that one for a session not on screen is refused and reaches the host as nothing, and that the later of two choices stays on screen when the host answers the earlier last; ui/scripts/drive-prototype.mjs sends, answers, cancels and answers trust through the window against the real host)`

<a id="HOSTCLIENT-4"></a>
### HOSTCLIENT-4: a send the host refuses leaves the message where the person wrote it

A send is made with the message as typed and the draft is cleared only once the host has taken it. A send during a turn is refused by the host as `turn_in_flight`, and the window says in words that a turn is already running and that the message stays, and keeps it in the box. The window queues nothing and makes no send of its own, so a turn that ends does not send what was refused. A message typed and not sent stays with its session when the window shows another or closes its view.

`verified-by: by-construction (ui/scripts/prototype-client.test.mjs holds that a send refused as busy comes back with the host's code and changes nothing in the state, and that the next send is made; ui/scripts/drive-prototype.mjs sends from the window while another client's turn runs, checks that the model service was never reached with that message and that the box still holds it, and sends it once the turn has ended, and checks that a message typed and not sent is still in the box after the view is closed and opened again)`

<a id="HOSTCLIENT-5"></a>
### HOSTCLIENT-5: closing a view or the window closes no session

The window offers no way to close a session. Closing its view of one sends nothing, and the session stays attached to this client's connection until the channel ends. Closing the window ends this client and nothing else: the host keeps each session and its work, and the other clients go on and see the turn finish.

`verified-by: by-construction (ui/scripts/prototype-client.test.mjs holds that closing the window's view sends no request and that opening the session again sends no second attach; ui/scripts/drive-prototype.mjs closes the window while another client's turn runs, and checks that the host went on, that the session is still listed as open and working, and that the other client sees the turn complete)`

<a id="HOSTCLIENT-6"></a>
### HOSTCLIENT-6: the window ends with the host's channel, and says so

When the host closes the channel, or the pipe to it fails, the window is told, says that the host ended the connection and why, offers nothing that needs it, and keeps what it last showed. It does not start an agent of its own in the host's place and it does not reconnect, because a program the host did not start cannot attach to a host ([U4](../design/mobile/open-questions.md#u4-attaching-a-client-the-host-did-not-launch)).

`verified-by: by-construction (ui/scripts/prototype-client.test.mjs closes the channel and holds that the state says it ended, with the reason, and that later actions are refused)`
