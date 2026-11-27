# Signing in to Gemini with a Google account, without the Google Cloud CLI

Route 2 of [#4683](https://github.com/brave/bravebot/issues/4683). This page records what the route
needs and the two decisions it waits for. Nothing on this page is built: no OAuth client id, no
loopback listener and no Google refresh token store exist in the code.

## What exists

Route 1 is built. `bravebot auth login google` runs `gcloud auth application-default login`,
records a `google-vertex` block that names a project and no key, and a turn sends the token
`gcloud auth application-default print-access-token` gives as a bearer to a Google Vertex host
([BACKEND-65](../specs/backends.md#BACKEND-65), [BACKEND-74](../specs/backends.md#BACKEND-74),
[BACKEND-75](../specs/backends.md#BACKEND-75)). It needs the Google Cloud CLI installed. bravebot
stores no Google credential: the Application Default Credentials are the CLI's.

## What route 2 would be

A sign-in that needs no `gcloud`. The program would do what the CLI does for a person, and keep the
result itself.

- **OAuth with PKCE on a loopback redirect.** The program opens Google's authorize page, receives
  the code and its `state` on a listener on `127.0.0.1`, checks `state`, exchanges the code with the
  PKCE verifier and receives an access token and a refresh token. This is the flow
  [connectors.md](connectors.md#credentials) describes as `pkce-loopback` for the Gmail connector
  (a loopback callback whose host the definition names, and a public client that sends no secret
  where the provider allows one). The listener, the `state` check, the refusal of a token answer
  with no refresh token and the refusal of a grant that lacks a requested scope would be the same
  rules, written once for the program and not copied from the connector vault.
- **The refresh token is kept in the credential store.** The sign-in would be one file in the state
  directory that the account owns, as `xai-account.json` is for the xAI sign-in
  ([XAI-5](../specs/xai-sign-in.md#XAI-5)) and `chatgpt-account.json` is for the ChatGPT sign-in
  ([CHATGPT-6](../specs/chatgpt-sign-in.md#CHATGPT-6)): created before anything is written to it,
  mode 0600 on Unix and an access-control list for one account on Windows with nothing inherited,
  written beside the file and renamed over it, a platform with neither refused, and no settings
  layer, `provider` block or environment variable holding a token or naming one. The access token
  is replaced using the refresh token before it expires, and a refresh the server refuses as no
  longer valid ends the sign-in, as those two clauses say for their servers. `auth status` and
  `auth logout` would report and remove that file in the way they do for those two sign-ins.
- **The access token goes only to the Google hosts compiled in.** A token for Gemini or Vertex AI is
  sent to the Google Vertex host that the project and location build, over https, and to no other
  host. A `baseURL` that names another host is sent a key or nothing, as it is for the gcloud token
  in [BACKEND-65](../specs/backends.md#BACKEND-65). The refresh token and the code go to Google's
  token endpoint alone. The destination is a constant of the program and not a value from settings,
  a model or the workspace, which is what [ROUTE-2](../specs/routing.md#ROUTE-2) requires of a
  routing field: derived from trusted input only, and never from fetched content.
- **The untrusted-content rule applies.** The token, the code and the account answer are
  carried and written, and no branch of the driver is decided by them
  ([AGENTS.md](../../agents/AGENTS.md)).

## Decisions a person makes first

The work does not start until a person has made both. They are not settled by anything in the
repository.

### Which OAuth client it would use

A program that runs a Google OAuth flow needs a client id issued by Google. The options known from
this repository:

1. **A client Brave registers for bravebot.** Brave would own it. Google's restricted-scope
   verification and, where restricted data is stored or sent through servers, a security assessment
   may apply, as [Gmail's OAuth client](connectors.md#gmails-oauth-client) records for
   `gmail.modify`. Whether the scope a Gemini or Vertex AI request needs is restricted, and what
   verification it needs, is not recorded in this repository. In Testing, Google
   limits a client to a list of named test users.
2. **The client of another Google program.** [connectors.md](connectors.md#compared-with-mcp-servers)
   notes that the Gmail MCP server signs in with Google's Gemini CLI client through a
   Google-run service that holds its secret and sees each refresh. Using the same client from bravebot
   would present bravebot to Google as that program.
3. **The client of Antigravity (`agy`).** Antigravity is a candidate to weigh against option 2. Nothing
   in this repository describes Antigravity, so what its sign-in flow is, whether it has a client a
   third-party program may use, and whether it replaces Gemini CLI are unverified and need a person to
   confirm from Google's own documents.
4. **The person's own client.** The person registers a Desktop client in their own Google Cloud
   project and gives bravebot its id, as [connectors.md](connectors.md#compared-with-mcp-servers)
   describes for Gmail. This leaves no Brave-owned client and puts the registration on the person,
   who for Vertex AI already has a project.

### Whether Google's terms allow it

Whether Google's terms allow a third-party program to use a client of options 2 or 3 for Gemini or
Vertex AI requests is undecided. The terms are not summarised here. A
person reads the terms that apply to the client and the service, and decides whether the use is
allowed, before work starts. Options 1 and 4 move the question to the terms that govern a client
that Brave or the person registers, and those terms still need reading.

## What the decisions change

- A Brave-registered client (option 1) needs a client id compiled into the program, a verification
  or assessment owned by Brave, and a consent page that names Brave.
- Options 2 and 3 need the client id of another program and, for a confidential client, whatever
  secret or service that program uses. The routing and storage rules above are the same.
- Option 4 needs a setting or a prompt for the client id, held as the vault holds a client id, and
  no client id in the program.

## Follow-up

The route is tracked by [#4706](https://github.com/brave/bravebot/issues/4706) and starts once a
person decides it is allowed. The Known costs bullet in
[backends.md](../specs/backends.md#known-costs) records that route 2 is not built.
