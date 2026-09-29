---
sidebar_position: 1
title: AWS Bedrock
description: Reach models through your own AWS account, name more than three, and sign in when AWS has no session.
---

# Reaching a model through AWS Bedrock

Set these variables to reach models through your own AWS account:

| Variable | What it sets |
|---|---|
| `BRAVEBOT_USE_BEDROCK` | turns the backend on |
| `AWS_REGION` | which region to reach Bedrock in (**required** once it is on) |
| `AWS_PROFILE` | which profile names the credentials to sign with (optional) |
| `ANTHROPIC_DEFAULT_OPUS_MODEL` | the model the Opus tier names |
| `ANTHROPIC_DEFAULT_SONNET_MODEL` | the model the Sonnet tier names |
| `ANTHROPIC_DEFAULT_HAIKU_MODEL` | the model the Haiku tier names |

Three services can answer a request: the aichat endpoint Brave runs, AWS Bedrock through your own
AWS account, and an [OpenAI-compatible gateway](openai-compatible.md) you
configured. Every build can reach Brave; the other two are what you configure.

Each tier takes either a model id or an inference-profile ARN. With `AWS_PROFILE` unset the AWS CLI
resolves credentials as it would for any other command, which is what a machine on instance
credentials already relies on.

**Any model your account can reach, whoever makes it.** A request is built in the body Bedrock
states for every provider it hosts rather than in one provider's own, so a tier can name a Claude,
an OpenAI, a Nova or a Llama model, or an inference profile standing for one, and nothing here has
to work out which provider is behind it. Nothing local checks the name you set, and Bedrock refuses
one your account cannot reach.

**The tier words stay `opus`, `sonnet` and `haiku`.** They name a slot in your configuration rather
than a model family, so a tier is whichever model you pointed it at.

**A tier you do not name is left out rather than guessed at.** An ARN cannot be derived from a model
name. Set one tier and one tier is offered.

**Configuring Bedrock takes nothing away from Brave.** Both rosters are offered together in `/model`,
so this adds models rather than replacing them. It also does not move the default: what answers when
nobody has chosen stays what it was.

**The model names the service.** A request goes to whichever service offers the model it names, and
nothing else participates: not which configuration is present, not which service answered last.
Bedrock refuses a model it does not recognise rather than substituting one, and the aichat endpoint has
never heard of an inference-profile ARN.

Your tiers sit under a heading reading `Bedrock, your my-profile AWS profile`, or
`Bedrock, your AWS account` with no profile set. The profile is named because it is what decides which
credentials sign the request, and because Brave serves part of its own roster through Bedrock too.
Every configured tier is marked free: premium means a Leo subscription, and reaching a model through
your own account does not involve one.

There is no automatic entry among them. On the Brave roster it means "let the server choose", which
Bedrock does not offer. A request names one model and gets it or an error.

If one service cannot say what it offers, the models known from your configuration alone are still
offered; a choice is refused only when nothing is left to choose. That is the position somebody
offline is most likely to be in.

### Naming more than three models

The tier variables name three models. A `provider` block keyed `amazon-bedrock` names as many as
your file lists, each under the id a request sends:

```json
{
  "provider": {
    "amazon-bedrock": {
      "options": { "region": "us-west-2", "profile": "my-profile" },
      "models": {
        "openai.gpt-5.6-sol": {},
        "arn:aws:bedrock:us-west-2:…:application-inference-profile/abc": {
          "name": "Sol on Bedrock",
          "limit": { "context": 1050000, "output": 128000 }
        }
      }
    }
  }
}
```

`options.region` is **required**, for the reason `AWS_REGION` is: a guessed region is a request that
fails somewhere far from the mistake. An entry without one configures no service at all.
`options.profile` picks which credentials sign, exactly as `AWS_PROFILE` does, and is optional on the
same terms.

**There is no credential to name here.** Bedrock takes a signature over the request rather than a
bearer token, so this entry reads neither `env` nor `options.apiKey`. Which AWS credentials sign
comes from the profile, resolved at the moment a request needs it.

**This adds to the tier variables rather than replacing them.** A name either of them offers reaches
your account, and everything else on the page above holds unchanged: the models are offered in
`/model` alongside Brave's roster, the default does not move, and each is marked free.

A model named this way has no tier, so its picker row carries the `name` you gave it and falls back
to the id where you gave none. Write one: an inference-profile ARN is not a name anybody reads. A
model named here is also chosen by that id exactly as written, without the gateway's
[`id/name` prefix](openai-compatible.md#naming-one) in front of it.

`limit.context` states that model's window, which is worth setting here because the figure otherwise
assumed is [deliberately low](#the-assumed-context-window). Following opencode, it needs `output`
beside it or it is not read, and `output` is itself
[how far a reply may run](../configuration.md#how-long-a-reply-may-run).

### Signing in

Where AWS has no usable session, the sign-in happens **before the turn starts**, and only for the
service the next request will actually go to. A turn served entirely by Brave never stops to
authenticate against AWS. The URL and code the AWS CLI prints appear line by line where you are
already reading, because collected up and printed at the end they would arrive once the code had
stopped working.

Credentials are resolved by running the AWS CLI, which is the tool you already sign in with. It holds
short-lived keys that expire during a session. `aws sso logout` clears them, and it takes no option to
narrow itself: it removes every cached token, so other tools sharing that cache need a fresh
`aws sso login` afterwards.

### The assumed context window

Every configured tier is assumed to have a 131,072-token window. Nothing at AWS reports a context
window, and an inference-profile ARN does not say which model it resolves to, so one deliberately low
figure stands in for all of them. Being wrong upward would stop the shortening of a conversation
altogether: every round asks, no round qualifies, and the session runs to exhaustion. Set
`BRAVEBOT_CONTEXT_BUDGET` if you know your model's real window and want to use it, or state it per
model with [`limit.context`](#naming-more-than-three-models).
