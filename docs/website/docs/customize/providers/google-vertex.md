---
sidebar_position: 3
title: Google Vertex AI
description: Configure Gemini through Google Vertex AI using an API key or Google Cloud credentials, with project-based access or project-free express mode.
---

# Reaching Google Vertex AI

## Setting it up with `bravebot auth login`

```sh
bravebot auth login gateway google-vertex
```

asks for a Google Cloud project, and nothing typed is express mode, which needs only a key. With a
project it asks for a location, where nothing typed is `global`. It then asks for the key with nothing
shown as you type, keeps it in `~/.bravebot/gateway-keys.json`, and records the entry in
`~/.bravebot/settings.json`, so no block is written by hand:

- no project: `"provider": {"google-vertex": {}}`, reached in [express mode](#express-mode-with-a-key-and-no-project);
- a project: `"provider": {"google-vertex": {"options": {"project": "<project>", "location": "<location>"}}}`,
  reached as described below.

A project or location that holds a character Google does not allow in it is refused before the key is
asked for, and nothing is stored or written. The question for the key names the host the key is sent
to. Where a block already names `google-vertex`, or `GOOGLE_API_KEY` or `GOOGLE_CLOUD_PROJECT` configures
it, no question is asked and no entry is written. `bravebot auth login` lists the service as Gemini on
Google Vertex AI, and [Signing in](../signing-in.md#gemini-on-google-vertex-ai) describes the rest,
including `auth status` and `auth logout`. The sign-in stores a key and does not choose a model: name one
as `google-vertex/google/gemini-2.5-flash`. It does not start `gcloud`, so a project signed in with Google
Cloud credentials is set up with [`bravebot auth login google`](#signing-in-with-google-cloud-credentials-instead-of-a-key).

## Reaching it by writing the block

Vertex AI is reached with an API key in one of two ways, and whether the entry states a project picks
between them. With a project, the entry reaches Vertex AI's OpenAI-compatible endpoint:

```json
{
  "provider": {
    "google-vertex": {
      "env": ["GOOGLE_API_KEY"],
      "options": { "project": "<your project id>", "location": "global" }
    }
  },
  "model": "google-vertex/google/gemini-2.5-flash"
}
```

The host is built from `project` and `location`, which is `global` where you state none. A `project`
holding a character a project id cannot hold, such as `/` or `@`, configures nothing rather than
sending your key somewhere else. A stated `baseURL` still wins.

The key is read from the variable `env` names and sent in the `x-goog-api-key` header, which is the
only place Google reads it from. Export it as `GOOGLE_API_KEY=<your key>`, or name another variable in
`env`. A block naming no `env`, no `options.apiKey` and no `options.keyFile` sends no key, and Vertex AI refuses it.

With no block, exporting `GOOGLE_API_KEY` and `GOOGLE_CLOUD_PROJECT` is enough, and `VERTEX_LOCATION`
sets a location other than `global`. A block replaces this. The service is only asked once a model
named `google-vertex/...` is chosen. `GOOGLE_API_KEY` is a name other Google tools read as well, so
the key you exported for one of them is what is sent here once you choose such a model.

Vertex AI has no model listing a key can call, so `/model` offers a short list of Gemini models
built into bravebot, which `bravebot doctor` names. A block that lists `models` is offered those
instead. No preview model is on the built-in list, because Google withdraws previews without notice.
The list is what the `global` location serves, and another location may not serve all of it. A model
on the built-in list is named with the id in front, as `google-vertex/google/gemini-2.5-pro`, in
`--model` or the `model` key, and one off it is named the same way, as
`google-vertex/google/gemini-3-flash-preview`.

## Signing in with Google Cloud credentials instead of a key

A project whose organisation disallows API keys is reached with the Google Cloud CLI's Application
Default Credentials. This needs [`gcloud`](https://cloud.google.com/sdk/docs/install), a Google Cloud
project that has the Vertex AI API enabled, and a role on it that may call the API, such as Vertex AI User.
No key is stored. Sign in and record the project with:

```sh
bravebot auth login google
```

It asks for the project at the terminal, runs `gcloud auth application-default login` and shows the address
and code the CLI prints, then adds the block below to `~/.bravebot/settings.json` with `location` set to
`global`. A project that holds a character a project id cannot hold is refused before `gcloud` starts, and a
sign-in that fails or is cancelled records nothing. Where `gcloud` already gives a token, no browser opens.
Where a block or `GOOGLE_CLOUD_PROJECT` already configures the service, the file is left as it is and the
command says so. It is refused in an incognito session, since it writes the settings, and without a
terminal, since the project is typed. [Signing in](../signing-in.md#google) has the full description.

`bravebot auth status google` says whether `gcloud` gives a token without starting a sign-in, and exits 0
only if it does. `bravebot auth logout google` removes the block the sign-in wrote, only where it still holds
exactly that, and leaves the Google Cloud CLI's sign-in in place. Other programs on the machine read the same
credentials, so it asks at a terminal whether to end them too, and only a yes runs
`gcloud auth application-default revoke --quiet`. A script ends them with `gcloud auth application-default
revoke`.

The block, which you can also write by hand after running `gcloud auth application-default login`:

```json
{
  "provider": { "google-vertex": { "options": { "project": "<your project id>" } } },
  "model": "google-vertex/google/gemini-2.5-flash"
}
```

With no block, exporting `GOOGLE_CLOUD_PROJECT` and no `GOOGLE_API_KEY` is the same. Where no key is
found, bravebot runs `gcloud auth application-default print-access-token` and sends the result as a
bearer token. A file `options.keyFile` names that gives no key counts as no key found, as an unset
variable does. If `gcloud` gives no token, bravebot runs `gcloud auth application-default login` before
the first request of a turn and shows the address and code it prints. `GOOGLE_APPLICATION_CREDENTIALS`
and impersonation are read by `gcloud` and not by bravebot. bravebot does not pass `gcloud` a
`GOOGLE_APPLICATION_CREDENTIALS` that names a file in a directory a stage may write, or a relative
or missing path, since a credential file of type `external_account` runs a command. It leaves off
the other variables `gcloud` and Python read to choose code to run under the same rule: `PYTHONPATH`
(a list, kept only when every entry passes), `PYTHONHOME`, `PYTHONUSERBASE`, `PYTHONPYCACHEPREFIX`,
`BROWSER`, `OPENSSL_CONF` and the two `CLOUDSDK_CONTEXT_AWARE_*` file paths. `CLOUDSDK_PYTHON_ARGS`
is always left off. `CLOUDSDK_PYTHON` (the interpreter `gcloud` runs) and
`CLOUDSDK_CONFIG` (whose `virtenv/bin/activate` it sources) are kept only where they are an absolute
path outside every directory a stage may write. A bare interpreter name for `CLOUDSDK_PYTHON` is kept,
and a relative or missing path is left off. If your credential file or interpreter lives in a directory
a stage may write, it is left off and `gcloud` signs in as it would with none set. The token goes only to Google's Vertex
host: an entry with a `baseURL` that names another host, or with an `authorization` header of its own,
is never sent one. A key that is found is used and `gcloud` is not started. bravebot runs `gcloud` for
each request, since the command states no expiry to keep an answer until.

## Express mode, with a key and no project

An entry that states no `project` is reached in Vertex AI express mode, through Google's native
`generateContent` API. It needs the key and nothing else:

```json
{
  "provider": {
    "google-vertex": {
      "env": ["GOOGLE_API_KEY"]
    }
  },
  "model": "google-vertex/google/gemini-2.5-flash"
}
```

With no block, exporting `GOOGLE_API_KEY` alone is enough. The request goes to
`aiplatform.googleapis.com` with no region in the address, and Google chooses where it is served, so
`location` and `VERTEX_LOCATION` are not read. State a `project` to reach a particular region. An
entry that states a `baseURL` and no project keeps speaking the OpenAI-compatible protocol to it,
unless it also states `"npm": "@ai-sdk/google-vertex"`, which says the host takes Vertex's own API.
The key is sent in `x-goog-api-key`, as above. Tool calls work, and the signature Gemini 3 models attach to a
call is sent back with it. A model is named as `google-vertex/<publisher>/<model>`, and the publisher
is `google` for the Gemini models. The thinking level you choose with `/effort` is sent as a thinking
budget, and a model that refuses one is sent none. Express mode takes a key only. State a `project` to
sign in with Application Default Credentials.

Exporting `GOOGLE_API_KEY` for another Google tool also configures this service, and takes the
variable out of the environment of the programs bravebot starts.
