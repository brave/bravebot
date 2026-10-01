---
sidebar_position: 1
title: Connecting Gmail
description: Read Gmail from a session through Google's Workspace MCP server, with every other Google service switched off.
---

# Connecting Gmail

This page assumes you have read [MCP servers](../mcp-servers.md), which covers
[adding a server](../mcp-servers.md#adding-one), [approving it](../mcp-servers.md#approving-one)
and [what a server can reach](../mcp-servers.md#what-a-server-can-reach).

Google publishes a [Workspace server](https://github.com/gemini-cli-extensions/workspace)
(Apache-2.0, listed in [google/mcp](https://github.com/google/mcp)) that reads Gmail. It runs on
Node.js, so `node` has to be on your `PATH`. This recipe installs a released version by checking
its digest, so no package manager runs, and turns off everything except reading mail.

1. Download release v0.0.8 into a directory of its own, check the digest, and unpack it there. The
   server writes its token into this directory, so choose one nothing else uses:

   ```sh
   mkdir -p ~/google-workspace-mcp && cd ~/google-workspace-mcp
   curl -fLO https://github.com/gemini-cli-extensions/workspace/releases/download/v0.0.8/darwin.google-workspace-extension.tar.gz
   echo "ca53101fd8b355d7710ffc28d55b4f558df621861ff8cdb9e5e426987f290e80  darwin.google-workspace-extension.tar.gz" | shasum -a 256 -c
   tar -xzf darwin.google-workspace-extension.tar.gz && rm darwin.google-workspace-extension.tar.gz
   ```

   On Linux, download `linux.google-workspace-extension.tar.gz` and check it against
   `58e440542330f7f32906b0e1ec5171778b1d82c5343e690c611b76b940460cb7` with `sha256sum -c`.

2. Sign in once, outside bravebot, asking Google for Gmail read access and nothing else. The
   `WORKSPACE_FEATURE_OVERRIDES` value switches off every other group, and the same value goes into
   the declaration in step 3, so run both steps in one shell:

   ```sh
   GMAIL_ONLY='docs.read:off,docs.write:off,drive.read:off,drive.write:off,calendar.read:off,calendar.write:off,chat.read:off,chat.write:off,gmail.write:off,gmail.downloadAttachment:off,people.read:off,slides.read:off,sheets.read:off,time.read:off'
   GEMINI_CLI_WORKSPACE_FORCE_FILE_STORAGE=true WORKSPACE_FEATURE_OVERRIDES="$GMAIL_ONLY" \
     node dist/headless-login.js
   ```

   It prints a Google address. Its `scope` parameter is
   `https://www.googleapis.com/auth/gmail.readonly`, so check that before you sign in. Open it in
   any browser, sign in, and paste the credentials the page shows back into the terminal. The
   token is saved as `gemini-cli-workspace-token.json` in the directory, encrypted with a key kept
   beside it in `.gemini-cli-workspace-master-key`.

3. Declare it, naming the directory so the server may write its token there:

   ```sh
   bravebot mcp add gmail -s user \
     -e GEMINI_CLI_WORKSPACE_FORCE_FILE_STORAGE=true \
     -e WORKSPACE_FEATURE_OVERRIDES="${GMAIL_ONLY:?run step 2 in this shell first}" \
     -e BROWSER=www-browser \
     --dir ~/google-workspace-mcp \
     -- node ~/google-workspace-mcp/dist/index.js
   ```

   ```
     gmail   stdio   node /Users/you/google-workspace-mcp/dist/index.js
             variables: BROWSER (stored), GEMINI_CLI_WORKSPACE_FORCE_FILE_STORAGE (stored), WORKSPACE_FEATURE_OVERRIDES (stored), PATH
             directory, which it may write: /Users/you/google-workspace-mcp
             digest: 40ec293a
   ```

   It then asks whether to use the server. Answer yes once the lines above are what you expect.

   `HOME` is a directory of the server's own, so the token has to sit in the server's directory,
   and `--dir` is what makes that directory writable and the place it starts. `BROWSER=www-browser`
   stops the server opening a browser of its own when the token is missing or refused: it answers
   the call with `Please run: node dist/headless-login.js` instead. `-s user` asks for it in every
   session once you answer yes.

4. Refuse the two tools that send mail, in `~/.bravebot/settings.json`:

   ```json
   {
     "permissions": {
       "deny": [
         "Mcp(gmail:gmail_send)",
         "Mcp(gmail:gmail_sendDraft)"
       ]
     }
   }
   ```

   With the write group off the server does not offer either tool, so these rules matter on the day
   someone edits the declaration and drops `gmail.write:off`. A `deny` rule holds where no question
   is asked, answer 2 and `--dangerously-skip-permissions` included. The server has no tool that
   forwards mail or creates a filter.

The session then offers `gmail:gmail_search`, `gmail:gmail_get` and `gmail:gmail_listLabels`, and
the two tools every server of this kind carries, `gmail:auth_clear` and `gmail:auth_refreshToken`.
What they return is private and quarantined, so the model reads a message only through a processor
or after you let it out of quarantine (see [vetting](../../security/vetting.md)).

What you have handed over:

- **The token reads the whole mailbox.** `gmail.readonly` covers every message and label, password
  reset links included. It cannot send, change or delete mail. Taking it back is removing the app
  in your [Google account permissions](https://myaccount.google.com/permissions) and deleting
  `gemini-cli-workspace-token.json` and `.gemini-cli-workspace-master-key`.
- **A Google-run service sees your tokens.** Sign-in and the hourly refresh go through
  `google-workspace-extension.geminicli.com`, a service that holds the app's client secret. It
  receives the authorisation code, returns the tokens, and receives the refresh token each time
  the access token is renewed. The domain is registered to Google LLC. bravebot cannot check what
  the service does with them.
- **The key sits beside the token.** Anything that can read `~/google-workspace-mcp` can decrypt
  the token, and the server can read all of it.
- **An attachment cannot be saved.** `gmail_downloadAttachment` writes to any absolute path the
  call names, and the directory the server may write holds its own code, so
  `gmail.downloadAttachment:off` in the override string keeps it out of the list.
- **You update it yourself.** The directory holds the release you checked. A newer release is
  downloaded and checked the same way.
