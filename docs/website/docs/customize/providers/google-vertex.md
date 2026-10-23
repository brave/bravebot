---
sidebar_position: 3
title: Google Vertex AI
description: Reach Gemini models through Vertex AI with an API key, with or without a project, and what express mode is.
---

# Reaching Google Vertex AI with an API key

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
`env`. A block naming no `env` and no `options.apiKey` sends no key, and Vertex AI refuses it.

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
`google-vertex/google/gemini-3-flash-preview`. Signing in with Google Cloud credentials instead of a
key is not supported.

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
budget, and a model that refuses one is sent none. Application Default Credentials, service-account
files and `gcloud auth` tokens are not supported.

Exporting `GOOGLE_API_KEY` for another Google tool also configures this service, and takes the
variable out of the environment of the programs bravebot starts.
