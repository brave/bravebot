---
sidebar_position: 2
title: Connecting Google Calendar
description: Read Google Calendar from a session through Google's Workspace MCP server, with every other Google service switched off.
---

# Connecting Google Calendar

This page assumes you have read [MCP servers](../mcp-servers.md), which covers
[adding a server](../mcp-servers.md#adding-one), [approving it](../mcp-servers.md#approving-one)
and [what a server can reach](../mcp-servers.md#what-a-server-can-reach). It uses the same server
as [Connecting Gmail](gmail.md), so the reasons for each step are given there.

Google publishes a [Workspace server](https://github.com/gemini-cli-extensions/workspace)
(Apache-2.0, listed in [google/mcp](https://github.com/google/mcp)) that reads Google Calendar. It
runs on Node.js, so `node` has to be on your `PATH`. This recipe installs a released version by
checking its digest, so no package manager runs, and turns off everything except reading calendars.

1. Download release v0.0.8 into a directory of its own, check the digest, and unpack it there. The
   server writes its token into this directory, so choose one nothing else uses. If you have also
   connected Gmail, do not reuse its directory: each directory holds one token.

   ```sh
   mkdir -p ~/google-workspace-calendar-mcp && cd ~/google-workspace-calendar-mcp
   curl -fLO https://github.com/gemini-cli-extensions/workspace/releases/download/v0.0.8/darwin.google-workspace-extension.tar.gz
   echo "ca53101fd8b355d7710ffc28d55b4f558df621861ff8cdb9e5e426987f290e80  darwin.google-workspace-extension.tar.gz" | shasum -a 256 -c
   tar -xzf darwin.google-workspace-extension.tar.gz && rm darwin.google-workspace-extension.tar.gz
   ```

   On Linux, download `linux.google-workspace-extension.tar.gz` and check it against
   `58e440542330f7f32906b0e1ec5171778b1d82c5343e690c611b76b940460cb7` with `sha256sum -c`.

2. Sign in once, outside bravebot, asking Google for Calendar read access and nothing else. The
   `WORKSPACE_FEATURE_OVERRIDES` value switches off every other group, and the same value goes into
   the declaration in step 3, so run both steps in one shell:

   ```sh
   CALENDAR_ONLY='docs.read:off,docs.write:off,drive.read:off,drive.write:off,calendar.write:off,chat.read:off,chat.write:off,gmail.read:off,gmail.write:off,people.read:off,slides.read:off,sheets.read:off,time.read:off'
   GEMINI_CLI_WORKSPACE_FORCE_FILE_STORAGE=true WORKSPACE_FEATURE_OVERRIDES="$CALENDAR_ONLY" \
     node dist/headless-login.js
   ```

   It prints a Google address. Its `scope` parameter is
   `https://www.googleapis.com/auth/calendar.readonly`, so check that before you sign in. Open it
   in any browser, sign in, and paste the credentials the page shows back into the terminal. The
   token is saved as `gemini-cli-workspace-token.json` in the directory, encrypted with a key kept
   beside it in `.gemini-cli-workspace-master-key`.

3. Declare it, naming the directory so the server may write its token there:

   ```sh
   bravebot mcp add calendar -s user \
     -e GEMINI_CLI_WORKSPACE_FORCE_FILE_STORAGE=true \
     -e WORKSPACE_FEATURE_OVERRIDES="${CALENDAR_ONLY:?run step 2 in this shell first}" \
     -e BROWSER=www-browser \
     --dir ~/google-workspace-calendar-mcp \
     -- node ~/google-workspace-calendar-mcp/dist/index.js
   ```

   ```
     calendar   stdio   node /Users/you/google-workspace-calendar-mcp/dist/index.js
                variables: BROWSER (stored), GEMINI_CLI_WORKSPACE_FORCE_FILE_STORAGE (stored), WORKSPACE_FEATURE_OVERRIDES (stored), PATH
                directory: /Users/you/google-workspace-calendar-mcp
   ```

   It then asks whether to use the server, and shows a digest line under these. Answer yes once
   the lines above are what you expect.

   `--dir` makes the server's directory writable and the place it starts, which is where the token
   has to sit. `BROWSER=www-browser` stops the server opening a browser of its own when the token
   is missing or refused: it answers the call with `Please run: node dist/headless-login.js`
   instead. `-s user` asks for it in every session once you answer yes.

4. Refuse the four tools that change a calendar, in `~/.bravebot/settings.json`:

   ```json
   {
     "permissions": {
       "deny": [
         "Mcp(calendar:calendar_createEvent)",
         "Mcp(calendar:calendar_updateEvent)",
         "Mcp(calendar:calendar_respondToEvent)",
         "Mcp(calendar:calendar_deleteEvent)"
       ]
     }
   }
   ```

   With the write group off the server does not offer any of them, so these rules matter on the
   day someone edits the declaration and drops `calendar.write:off`. Creating or updating an
   event can invite other people, and that sends them an email. A `deny` rule holds where no
   question is asked, answer 2 and `--dangerously-skip-permissions` included.

The session then offers `calendar:calendar_list`, `calendar:calendar_listEvents`,
`calendar:calendar_getEvent` and `calendar:calendar_findFreeTime`, and the two tools every server
of this kind carries, `calendar:auth_clear` and `calendar:auth_refreshToken`. What they return is
private and quarantined, so the model reads an event only through a processor or after you let it
out of quarantine (see [vetting](../../security/vetting.md)).

What you have handed over:

- **The token reads every calendar you can see.** `calendar.readonly` covers all events on your
  own calendars and on calendars others have shared with you, including titles, attendees, meeting
  links and locations. It cannot create, change or delete an event. Taking it back is removing the
  app in your [Google account permissions](https://myaccount.google.com/permissions) and deleting
  `gemini-cli-workspace-token.json` and `.gemini-cli-workspace-master-key`.
- **A Google-run service sees your tokens.** Sign-in and the hourly refresh go through
  `google-workspace-extension.geminicli.com`, a service that holds the app's client secret. It
  receives the authorisation code, returns the tokens, and receives the refresh token each time
  the access token is renewed. The domain is registered to Google LLC. bravebot cannot check what
  the service does with them.
- **The key sits beside the token.** Anything that can read `~/google-workspace-calendar-mcp` can
  decrypt the token, and the server can read all of it.
- **You update it yourself.** The directory holds the release you checked. A newer release is
  downloaded and checked the same way.
