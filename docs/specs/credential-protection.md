---
id: CRED
title: Credential protection
status: proposed
governs:
  - crates/config/src/lib.rs
  - crates/config/src/env_var.rs
  - crates/config/src/provider.rs
  - crates/config/src/settings.rs
  - crates/ui-bridge/src/settings.rs
  - crates/bedrock/src/credentials.rs
  - crates/core/src/credentials.rs
  - crates/sandbox/src/crash.rs
guards:
  - symbol: Secret::expose
documented-by:
  - docs/website/docs/customize/configuration.md
  - docs/website/docs/security/security.md
---

## Scope

The credentials already on a person's machine, the ones bravebot adds, and what an agent does with
a secret: what it can end up holding, which of those it should hold at all, where the ones it must
hold are kept, what a grant binds, and who enforces each bound.

The typed credential value does not become a labelled value, which is the split that puts this in a
file of its own. Labelled content may nevertheless contain credential material.





## Where credentials come from

Four tables, split by location and then by author.

### Already on the machine, outside the working tree

Present whether or not an agent ever runs. 

| Group | Where it lives |
| --- | --- |
| OS stores | `~/Library/Keychains/login.keychain-db`; `%APPDATA%\Microsoft\Credentials`, `\Protect`; `~/.local/share/keyrings/`; `~/.password-store/`; `keyctl` session |
| Shell & env | `~/.bashrc`, `.zshrc`, `.zshenv`, `.profile`, `.bash_profile`; `.bash_history`, `.zsh_history`, `.psql_history`, `.mysql_history`, `.node_repl_history`, `.dbshell`; agent env, including the `env` block of `~/.bravebot/settings.json` |
| SSH | `~/.ssh/id_rsa`, `id_ed25519`, `id_ecdsa`, custom names; `~/.ssh/config`; `*.ppk`; `\\wsl$\...\home\<user>\.ssh` |
| Git | `~/.git-credentials`; `~/.gitconfig`; `~/.netrc`, `_netrc`; `~/.config/gh/hosts.yml`, `~/.config/glab-cli/config.yml` |
| Cloud | `~/.aws/credentials`, `~/.aws/config`, `AWS_*` env; `~/.aws/sso/cache/*.json`, `~/.aws/cli/cache/`; `~/.config/gcloud/credentials.db`, `access_tokens.db`, `application_default_credentials.json`, SA JSON; `~/.azure/msal_token_cache.json`, `azureProfile.json`; `~/.config/vercel/auth.json`, `~/.config/netlify/config.json`, `~/.fly/config.yml`, `~/.config/doctl/`, `~/.supabase/`; `http://169.254.169.254`, Codespaces token endpoint |
| Containers | `~/.docker/config.json`, `~/.config/containers/auth.json`; `~/.kube/config`, `~/.kube/cache/`; `/var/run/secrets/kubernetes.io/serviceaccount/token`; `~/.config/helm/repositories.yaml` |
| Registries | `~/.npmrc`, `~/.yarnrc.yml`; `~/.pypirc`, `~/.config/pip/pip.conf`; `~/.m2/settings.xml`, `~/.gradle/gradle.properties`; `~/.cargo/credentials.toml`, `NuGet.Config`, `~/.composer/auth.json`, `~/.gem/credentials`; `~/.cache/huggingface/token`, `GOPRIVATE` + netrc |
| IaC & secrets | `~/.terraformrc`, `~/.terraform.d/credentials.tfrc.json`; `~/.pulumi/credentials.json`; `~/.vault-token`; `~/.config/sops/age/keys.txt`, `*.agekey`, vault password files; `.gitlab-runner/config.toml` |
| Databases | `~/.pgpass`, `~/.my.cnf`, `~/.mylogin.cnf`; DBeaver `credentials-config.json`, JetBrains password safe, TablePlus, pgAdmin `pgadmin4.db` |
| IDE & agent | `claude_desktop_config.json`; another agent's own auth: `~/.claude/.credentials.json` at mode 0600, or the login keychain item it falls back from; `settings.json`, Settings Sync, extension storage; `~/.config/JetBrains/*/options/`; agent transcripts / tool logs: session dirs, undo history; `~/.config/github-copilot/hosts.json` |
| Browser | `Login Data`, `Cookies`, `Web Data`, `Local State`; `logins.json`, `key4.db`, `cookies.sqlite`; `userDataDir`, `storageState.json` outside the tree |
| Certs & signing | `*.pem`, `*.key`, `*.p12`, `*.pfx`, `*.jks`, `*.keystore` outside the tree; mkcert `rootCA-key.pem`; `~/.android/debug.keystore`, release keystores; `~/Library/MobileDevice/Provisioning Profiles/`, App Store Connect `.p8`; code-signing certs: OS keychain / cert store |
| Network & host | `*.ovpn`, `~/.config/openconnect/`; SMB credential files under the home directory |
| API clients | `~/Postman/`, environment exports; `~/.config/Insomnia/`, `~/.config/Bruno/` |
| User files | `notes.txt`, `passwords.txt`, `creds.txt` on Desktop / Documents; KeePass `.kdbx`, Bitwarden `data.json`, Enpass |
| Indirect | `~/Downloads`, `~/Desktop`; memory artifacts: core dumps, swap / hibernation file, crash reports; Docker layers with `ARG`/`COPY .env`, published bundles |

**Some things in that space are not credentials and are listed nowhere above.**
`~/.ssh/known_hosts` and `authorized_keys` are a recon and persistence surface.
`/var/run/docker.sock` and `$SSH_AUTH_SOCK` are access paths rather than stored values: nothing is
handed over and nobody can refuse, which is the case [CRED-5](#CRED-5) takes off the scale.
`/proc/<pid>/environ`, a clipboard manager and a terminal's scrollback are channels a credential
passes through rather than places one is kept. And a mail store, a notes application, a synchronised
folder or a backup is where a credential *ended up*: that is the leakage row of the table below,
which owes detection rather than a tier.

Root-owned files are also absent, `/etc/shadow` and `/etc/NetworkManager/system-connections`
included. This agent runs as the person and cannot read them, so listing them would claim a reach
the design does not have.

### Added by the user, in the working tree

Committed or created by hand. An entry marked `(either)` is one an agentic turn also produces in the
ordinary course, and a turn that writes one is attributed to the turn rather than to the person.

| Group | Where it lives |
| --- | --- |
| App config | `.env`, `.env.local`, `.env.production`, `.env.test`; `.envrc`, `.envrc.local`; `config/*.yml`, `application.properties`, `settings.py`, `appsettings.json`; `database.yml`, `alembic.ini`, ORM config; project `.npmrc`, `.yarnrc.yml`, `pip.conf`, `.pypirc` (either) |
| Infra & deploy | `docker-compose.yml`, `docker-compose.override.yml`; `Dockerfile` `ENV` / `ARG`; `Secret` objects, `values.yaml`, kustomize overlays; `serverless.yml`, `vercel.json`, `fly.toml`, `netlify.toml` (either) |
| IaC | `*.tfvars`, `*.auto.tfvars`; inventory files, `group_vars/`, `values.yaml` (either) |
| CI/CD | `.github/workflows/*.yml`, `.gitlab-ci.yml`, `.circleci/config.yml`; `scripts/*.sh`, `Makefile`, `justfile` (either) |
| Agent & IDE | `.mcp.json`, `.cursor/mcp.json`; `CLAUDE.md`, `.cursorrules`, `AGENTS.md`; `.vscode/launch.json`, `.vscode/settings.json`, `.idea/` (either) |
| Tests & fixtures | hardcoded keys in `conftest.py`, `*.spec.ts` (either). committed Postman / Insomnia exports, `*.http`, `*.rest` |
| Keys & certs | `*.pem`, `*.key`, `*.p12`, `*.jks` inside the repo (either). GCP SA key, `google-services.json`, Firebase config copied in |
| Docs & records | README and docs examples: real keys in code samples and quickstarts (either). `*.ipynb` source cells and saved outputs; `notes.md`, `TODO.md`, `setup.txt` |
| Byproducts | `.env.swp`, `.env.swo`, `.env.save` |
| Git directory | `.git/config`, whose remote URL may carry a token inline; committed objects: git objects, packfiles, reflog, stashes, tags; tracker text: commit messages, PR and issue bodies drafted from the tree (either) |

### Added by the agent, in the working tree

Only a turn puts these here. This is the half that can be enforced, because a finding in a diff a
turn just produced has exactly one author.

| Group | Where it lives |
| --- | --- |
| App config | `SECRET_KEY_BASE`, Django `SECRET_KEY`, `NEXTAUTH_SECRET`, `JWT_SECRET` |
| IaC | `terraform.tfstate`, `*.tfstate.backup` |
| Tests & fixtures | `storageState.json`, recorded auth state; `seeds/`, migrations creating DB users |
| Keys & certs | generated local certs: dev certs and local CA output written to the tree |
| Byproducts | `logs/`, `*.log`, `npm-debug.log`; `*.sql`, `*.dump`, `*.csv` written during a task; `.env.bak`, `config.yml.orig`, `*.orig` where a turn copied a file before editing it, which the edit tool does not do for it |


### Added by the agent, outside the working tree

What this agent keeps for itself, and what a turn brings into existence elsewhere. The first
group is the program: those paths are in this repository and nothing else writes them. The rest a
turn reaches by running a command, so they are what the tool surface makes possible rather than
features anything here implements.

| Group | Where it lives |
| --- | --- |
| Its own auth | the signing key and key id that sign a request to the default backend, baked into the binary at build time and masked rather than encrypted, so one build's key is every install's key; `~/.bravebot/leo-premium.json`, kept to the account that wrote it by a mode of 0600 on Unix and a protected one-account access-control list on Windows, the imported subscription's signed credential batch, deliberately not the OS keychain. With Bedrock opted into instead, the AWS CLI's own cache, listed under Cloud above |
| What it keeps beside that | `~/.bravebot/history`, every submitted prompt across runs; `~/.bravebot/sessions/<project-key>/<id>.json` and its trail beside it, and the record holds the conversation rather than a title, so it carries whatever file content the planner was shown; `/export`, which writes a transcript as markdown inside the working directory |
| Its own project settings | `.bravebot/settings.json` and `.bravebot/settings.local.json` in a checkout, a layer over the one at home, which on some machines carries credentials |
| Minted access, by a turn | `~/.ssh/`, then deploy key or `authorized_keys`; `gh auth login`, `~/.config/gh/hosts.yml`; `aws iam create-access-key` → `~/.aws/credentials`; `gcloud iam service-accounts keys create`; `az ad sp create-for-rbac`; `npm token create`, `docker login`, `helm registry login` |
| Generated by a turn, stored outside the tree | `~/.ngrok2/ngrok.yml`, `~/.cloudflared/*.json`; `mkcert -install` |
| Pushed to a remote by a turn | `gh secret set`, GitLab CI variables; PaaS env vars: Vercel, Netlify, Fly; k8s `Secret` objects, Vault paths; webhook registration: third-party SaaS |



## How to protect each of these

Five answers, strongest first. Take the strongest the far end allows. Anything weaker is a
deliberate, recorded choice, and the walk below is what records it.

**1. Not read at all.** The value stays where it is and nothing brings it into the agent. This is
the only answer that costs no one a decision, and the only one that works before anybody is asked.
Everything outside the working tree has it already: `read_file`, `write_file`, `edit_file` and
`search` resolve a path and refuse one that lands outside, symlinks included. Its strict form
covers what the agent authenticates with, `~/.bravebot/leo-premium.json` and the credentials that
sign a model request, which CRED-14 puts out of reach of every program, prompt and record with no
legitimate exception. Two things get past it: a command a turn runs, and `/add-dir`, which makes a
named directory reachable for the session.

**2. Something else performs the action.** The agent asks for an outcome and never sees a secret.
A hardware key signs and a touch refuses; a signing agent with per-use confirmation does the same;
a broker sends the mail or calls the API and reports what happened. This is the strongest answer
that still gets work done, and it needs a performer to exist for the operation, which is the only
reason most of the inventory does not have it.

**3. A placeholder, substituted at the boundary.** For actions only the agent can perform, such as
`git push` or an arbitrary HTTP call, the agent holds a stand-in and something on the way out
swaps in the real value for one named destination. The agent still holds nothing, so on custody
this is what performing the action is, and on authority it is not: the boundary bounds where a
request may go and where the value may appear in it, and does not bound which operation is asked
for. A performer exposes one action; a substituted credential exposes every action the far end
will accept at that destination. It costs a proxy, a local certificate authority, and a rule about
where in a request the placeholder may appear.

**4. A temporary credential, minted for the occasion.** A real secret, but bounded before it is
issued and ended by whoever issued it: an installation token for one repository, an assumed-role
session, a lease, a single-use batch. The bound has to be enforced by something the agent cannot
impersonate, and it has to have an end. Scope without an end is not this answer.

**5. A permanent credential.** A bearer secret that sits here indefinitely. This is where most
of the inventory sits today, and it is a position rather than a protection: what is owed is
detection, because the credential outlives every decision made about it. Leakage happens too: the
tree may hold credentials nobody decided anything about, and what a turn writes needs scanning.

### Which of these each group can reach

| group | best of answers 2 to 5 | what stops it there |
| --- | --- | --- |
| hardware keys and tokens: a security key, a TPM, a smart card | performed | nothing. The key cannot leave, the agent asks for a signature, and a touch is a refusal |
| signing agents: `ssh-agent`, `gpg-agent` | performed, with per-use confirmation | without confirmation nothing is handed over and nobody can refuse, which is off the scale rather than protected |
| SaaS API keys: payments, comms, workspace, observability, model providers | performed | a performer has to exist for the operation. The protocol is ordinary and the payload is readable, so this is a question of building it |
| site logins and session cookies | performed | a performer that fills the form on the credential's own origin. A grabbed cookie is a bearer token and is permanent |
| databases | performed | a proxy can mediate because it may read the query. Where it may not, the payload condition fails |
| `git push` over HTTPS, arbitrary HTTP | placeholder | nothing can perform these whole, so substitution at the boundary is the ceiling |
| cloud: static keys and CLI caches | temporary | the issuer mints assumed sessions with a policy the agent cannot widen, and ends them. Performing needs something that signs each request |
| forge access: a personal token, a CLI login | temporary | an installation token for one repository is the bounded derivative. Performing fails only for want of a performer |
| secret managers and vaults | temporary | a lease is the bound and the issuer enforces it. What is behind the lease starts its own walk |
| container registries and package registries | permanent, usually | the counterparty refuses. Most registries have no derived form, and many will not expire a publish token |
| code signing and platform keystores | performed where the key is in hardware, permanent where it is a file | decided by where the key lives, not by the protocol |
| project and infrastructure config in the tree | whatever the underlying credential reaches | the file is a location, not an issuer |
| sockets and metadata endpoints: a container socket, an instance metadata service | none of these | nothing is handed over and nobody can refuse, so there is no custody to bound. Answered where the capability is granted |
| leakage: histories, packfiles, transcripts, dumps, backups, swap | none of these | not credentials. They are where a permanent one ended up, and they are why detection is owed |

### How the walk decides which you get

Answers 2 to 5 are the four tiers, and three gates separate them. Start at the top and walk down.
Failing a gate drops exactly one tier, nothing skips a tier, and nothing climbs back without the
arrangement changing.

```
                    Delegated
                        │
   gate 1 ── can something the agent cannot impersonate decide each use, and refuse?
        │ no, and nothing is handed over ──→ off the scale: unbounded authority
        ↓ no, and material must be handed over
                     Granted
                        │
   gate 2 ── can the issuer mint a bounded derivative, enforced beyond the agent's reach?
        ↓ no
                   Held briefly
                        │
   gate 3 ── can the issuer mint on demand and enforce death, unrefreshable without authority?
        ↓ no
                      Held
```

**Delegated** covers answers 2 and 3, because in both the agent holds nothing and something it
cannot impersonate decides every use. **Granted** and **Held briefly** are both answer 4, differing
in whether the bound or the lifetime is what the issuer enforces. **Held** is answer 5.

**The obligations ratchet downward.** Each drop inherits everything owed below it, so Held carries
all three. That accumulation is what makes a drop a cost rather than a shrug.




### Where the design lives

| answer | what delivers it |
| --- | --- |
| 1. not read at all | the workspace boundary in this repository: every tool that names a file resolves the path and refuses one outside the tree. Past that, [sandboxing.md](sandboxing.md) specifies credential scopes but holds only for the stdio servers it covers, and says itself that nothing in that section binds a `run` until a profile exists |
| 2. something else performs | [brokering](../design/credential-brokering.md): the authority holds the credential and carries out the action whole. |
| 3. a placeholder | [brokering](../design/credential-brokering.md): a stand-in the agent holds, swapped at the egress boundary for one named destination|
| 4. a temporary credential | [brokering](../design/credential-brokering.md): something bounded and expiring, with the issuer enforcing both ends.|
| 5. a permanent credential | [discovery](../design/credential-discovery.md): find it before the tree is read, show it, and carry a disposition that outlives the run. |

**Only one route runs upward.** A credential found at answer 5 reaches answer 2 through discovery's
enrol disposition, and only where a performer exists for the operation. Deny, remove and accept all
stay at 5. Nothing else in this document moves a credential from a weaker answer to a stronger one.


## Clauses

Everything above is commentary. These are the rules.

<a id="CRED-1"></a>
### CRED-1: a secret never becomes a labelled value except through one named exposure

Nothing the type carrying a credential offers yields its bytes, and it offers no equality at
all. Printing it, converting it or serialising it does not, and both its debug and display output
redact. Where a credential genuinely has to be compared, the comparison is made in constant time by
the component that owns it, never by an operator on the type. One named call exposes the value, and
it is the only one.

This is a guarantee about a type, not about meaning. Ordinary labelled content can contain
credential material, which is what the tree inventory is a list of.

**Why.** A credential that could become a labelled value could be taint-joined, relabelled, or
handed to a gate that forgot the case. None of that is reachable if the conversion does not exist.

**Why no equality.** An equality operator answers a question about the bytes, so it recovers them a
guess at a time, and the derived form compares in time proportional to the shared prefix. The type
carrying a label already refuses equality for the same reason
([LABEL-4](labels.md#LABEL-4)); the type carrying a secret has the stronger claim to it.

`verified-by: bravebot_config::lib::secrets_are_redacted_in_debug_and_display`
`verified-by: by-construction (Debug and Display both redact; no Deref, AsRef, Borrow, Serialize or From impl or derive exists; equality is pinned by the compile_fail doctest on Secret in crates/config/src/lib.rs, since a derive is not spelled impl and a reader grepping for one would miss it)`

<a id="CRED-2"></a>
### CRED-2: every credential carries a tier, and the tier is where its gate walk stopped

A credential in use has a recorded tier, arrived at by walking down from Delegated. Where there is
custody and no recorded walk, the tier is Held, because that is the one that assumes nothing. A
credential nothing reads has no tier, and neither does the off-the-scale case CRED-5 covers.

**Why.** The walk is the only thing that distinguishes a credential somebody reasoned about from one
that ended up wherever it ended up. Defaulting the unexamined case to the top would let silence
claim the strongest tier.

`verified-by: none`

<a id="CRED-3"></a>
### CRED-3: a gate fails only for a stated reason, and dropping pays what the gate says it costs

Each drop records which of the gate's conditions failed, and whether the counterparty refused or
nobody attempted it. Those are different answers and they end at the same tier.

**Why.** Without the record a tier is an assertion. A gate the counterparty refused is a fact about
the world and a gate nobody attempted is a decision somebody made, and only the second is ours to
revisit.

`verified-by: none`

<a id="CRED-4"></a>
### CRED-4: nothing climbs a tier without the arrangement changing

A credential moves up only when what is true about it changes: a performer appears, an issuer offers
a bound, a value becomes mintable on demand. Rewriting a justification moves nothing.

**Why.** The gates ask about the arrangement, so only the arrangement answers them. A tier that can
be argued upward is a tier that will be.

`verified-by: none`

<a id="CRED-5"></a>
### CRED-5: authority with no custody and no refusal leaves the scale, and is declared where it is granted

Where nothing is handed over and nobody can refuse a use, the credential is on no tier. It is named
where the capability is granted and recorded when it is used.

**Why.** A logged-in tool, a live socket and a metadata endpoint have no custody to rank and no
bound to enforce, so placing them on the scale would put unbounded authority at the top of it. The
one thing available before confinement is that nobody grants it thinking they granted less.

`verified-by: none`

<a id="CRED-6"></a>
### CRED-6: a handle the agent can redeem alone is not a handle

Gate 1 passes only where redeeming what the agent holds needs the assent of something it cannot
impersonate. Where the agent can redeem it unaided, it is the credential under another name and the
gate has already failed.

**Why.** This is the test that keeps Delegated from being a relabelling exercise. Both answers that
reach it rest on what the agent holds being useless by itself.

`verified-by: none`

<a id="CRED-7"></a>
### CRED-7: a bound is decided before issue and enforced beyond the agent's reach

Gate 2 passes only where the bound on a derived artifact is fixed before it is issued and fails
closed where the agent cannot reach it. A bound the agent's own code checks is not a bound.

**Why.** A program running as the person presents itself as any other, so a check the agent could
impersonate answers to whoever is asking. A bound enforced past that point survives the agent's side
being wrong, which is the only reason a derivative is worth having.

`verified-by: none`

<a id="CRED-8"></a>
### CRED-8: a bound with no end does not pass gate 2

A derived artifact that is narrow and permanent does not pass. Narrowing is worth doing and does not
move the tier.

**Why.** Scope and time are separate properties and a permanent narrow secret is a static secret
with a small reach. Letting scope alone pass the gate would let gate 3's work be skipped by doing
gate 2's twice.

`verified-by: none`

<a id="CRED-9"></a>
### CRED-9: the issuer ends the credential's life, and refresh without authority is not expiry

Gate 3 passes only where the value is minted for a named step and killed by the issuer. A value the
agent can renew without further authority is permanent, whatever its stated lifetime.

**Why.** Refreshable-without-asking is the common false pass: a fifteen-minute token the agent
renews by itself is a permanent credential with extra steps, and belongs at Held.

`verified-by: none`

<a id="CRED-10"></a>
### CRED-10: a brief window is sized against detection, and the size is recorded

Where a credential is at Held briefly, the record says how quickly a leak would be noticed and acted
on. That figure does not establish the tier and does not move it.

**Why.** The issuer's enforcement is what bounds the credential, and it holds whether or not anybody
notices: a thirty-second token that cannot be refreshed is worth nothing on the thirty-first second
to an attacker nobody has detected. What detection decides is whether the window is short enough for
what the credential reaches, which is a judgement about this deployment rather than a fact about the
arrangement.

**Why it is not a tier test.** [CRED-2](#CRED-2) fixes the tier at where the gate walk stopped and
[CRED-4](#CRED-4) lets nothing move it but the arrangement. Response time is neither, so a tier that
depended on it could be lost by a rota change. The obligation stays because a window nobody sized is
a number somebody liked.

`verified-by: none`

<a id="CRED-11"></a>
### CRED-11: a turn does not copy a credential somewhere weaker than where it was

No turn writes a credential, or anything derived from one that would authenticate in its place,
anywhere less protected than where it already sits: the tree, a build or deployment file, an
instruction file the agent reads back, a pre-edit backup, a log, a transcript, a terminal, or a
commit. A person putting one somewhere is theirs to do and this does not reach it.

**Why.** This is the row of the tree inventory nothing else covers, because it is the one the agent
causes. Everything after it follows on its own: a secret in a file is read back by a later turn,
reaches the planner once the tree is vouched for, survives in a backup nobody deletes, and is pushed
like any other change.

`verified-by: none`

<a id="CRED-12"></a>
### CRED-12: withdrawn, replaced by CRED-13

Replaced by CRED-13, which owns creation. This stated that a credential a turn creates walks the
gates as it is created and is not created where the walk cannot be run. CRED-2 already requires a
recorded tier for every credential in use, and CRED-13 already refuses the creation where the value
cannot go to the authority, so this said the same thing a third time in weaker terms.

<a id="CRED-13"></a>
### CRED-13: a credential a turn creates is created into the authority

Where a turn brings a credential into existence, by asking a service to mint one, generating key
material, or choosing a secret a project will authenticate with, the value goes to the authority in
the same step and its tier is recorded then. It is not written to the tree, to a file under the
machine, to a record, or to a terminal, and what the tree receives is a reference. Where the value
cannot be created into the authority, the credential is not created and the turn says so.

**Why.** CRED-11 forbids copying a credential somewhere weaker than where it already sits, and a
credential a turn has just generated sits nowhere. There is no weaker place, so nothing reaches the
first place it lands. Creation is the only moment a value has no prior location, which is exactly
when a rule phrased around its prior location stops meaning anything.

**Why into the authority rather than merely not into the tree.** A generated key written to a file
on the machine is at Held before anyone has walked a gate, and the walk then ratifies where the
value already sits instead of deciding it. Creating into the authority is what leaves the walk
something to decide.

`verified-by: none`

<a id="CRED-14"></a>
### CRED-14: the credentials this agent holds for itself reach no program, no prompt and no record

What this agent authenticates with, and anything it holds to spend on the person's behalf, are
never written to a session record and never shown except as a redaction. Removing them from the
environment of a program the agent starts is the same rule [tools/run.md](tools/run.md) already
states and pins, covering the signing key and its key id, exempting a line the person typed at the
`!` prompt, and switched off only by an environment variable rather than by any settings file.

**Why not wider.** The person's own environment is left alone: asking a program to use credentials
they already have is an ordinary request, and a name filter cannot tell one of those from an
exfiltration.

**What it does not reach.** The subscription batch is stored as a plain string rather than in the
type CRED-1 governs, so a debug print of one is a debug print of live credentials. Where the backend
is reached through Bedrock, what this agent authenticates with is the machine's own cloud
credentials, and those are the ones the paragraph above deliberately leaves in place: the clause
holds for the signing key and does not hold for that configuration. Withholding there is a question
about the profile this agent resolved for itself, which it knows, rather than about a name. The CLI
that resolves them is a program this agent started all the same, so it is handed what the person's
own environment holds and none of what this agent authenticates with.

**The record is closed elsewhere.** No field of the trail can hold a credential, because
[TRACE-2](trace.md#TRACE-2) admits only a gate name, a capability, a label, a path or a slot id.
That is a stronger answer than a redaction rule and it is why this clause does not restate one; what
is owed there is the test, which scans the trail for every secret the process holds.

`verified-by: bravebot_bedrock::credentials::this_agents_own_credentials_reach_none_of_the_aws_cli_this_crate_starts`
`verified-by: bravebot_bedrock::credentials::the_machines_own_aws_configuration_still_reaches_the_cli`
`verified-by: bravebot_config::scrub::a_name_from_the_settings_file_is_not_one_of_this_agents_own`

<a id="CRED-15"></a>
### CRED-15: the tree is scanned before it is vouched for

The scan runs before the first turn touches a tree, and its findings are shown to the person before
anything in the tree can reach the planner. The question it runs ahead of is the one
[trust-map.md](trust-map.md) puts at startup.

**Why the ordering is the whole design.** After vouching, the tree's contents are disclosed to
whoever performs inference. A scan that runs later reports on a disclosure that already happened.

`verified-by: none`

<a id="CRED-16"></a>
### CRED-16: what a turn writes to the tree is scanned before the change is recorded as complete

The tree diff a turn produces is scanned with the same layers and the same finding shape as the
pre-run scan, under CRED-18 and CRED-19. A finding in that diff is attributed to the turn, is a
violation of CRED-11 or CRED-13, and the value does not remain in the tree.

**Why.** CRED-15 scans before the run, so the one thing this system causes is the only thing never
checked. The pre-run scan establishes what was already there; this one is the difference between a
credential the person left and a credential a turn wrote.

**Why attribution is better here than before.** An untracked `.env` found at the start has no blame
and no turn record, so `unknown` is a real answer. A diff names the turn that produced it, which is
why this scan can refuse where the other can only inform. It is not authorship: a turn that
reformats or moves a file already holding a key produces a diff carrying it without having written
it, and that case is reported rather than refused. Interactive whole-file writes use that
attribution only when the pre-image was trusted at capture. Untrusted prior bytes cannot excuse
a finding in a proposed trusted body.

**What is refused and what is asked about.** A value that declared itself a credential (a
provider's prefix over its own alphabet at its own length, or the password field of a URL) is
refused, and nobody is asked: there is no judgement to put to anybody, and a prompt that can be
answered "write it anyway" is a prompt a turn eventually gets past. A value inferred from a name
that sounds like a secret beside one that looks rare is raised on the approval the write already
needs, and the person decides. That inference catches a generated framework key, and it also
catches an inline Kubernetes `Secret`, a local development password and a test fixture; refusing on
all four with no override would stop ordinary work over a guess, and a scan people have to fight is
a scan they turn off. The prompt names the finding, so the question can be answered.

A finding raised this way goes to the person and never to the planner, which is what CRED-19
requires of a finding however it is answered.

`verified-by: bravebot_agent::turn::a_credential_a_turn_writes_never_reaches_the_tree`
`verified-by: bravebot_agent::turn::a_credential_pasted_by_an_edit_leaves_the_file_as_it_was`
`verified-by: bravebot_agent::turn::a_credential_the_file_already_held_does_not_refuse_the_change_carrying_it`
`verified-by: bravebot_agent::turn::what_the_scan_found_is_told_to_the_person_and_not_to_the_planner`
`verified-by: bravebot_agent::turn::a_value_that_only_looks_like_a_secret_is_put_to_the_person`
`verified-by: bravebot_agent::turn::the_prompt_says_which_value_it_is_asking_about`
`verified-by: bravebot_core::credentials::a_generated_key_is_recognised_from_its_name_and_its_rarity`
`verified-by: bravebot_core::credentials::a_provider_key_is_recognised_with_nothing_around_it_saying_so`
`verified-by: bravebot_core::credentials::a_provider_key_is_recognised_when_it_is_assigned_to_a_name`
`verified-by: bravebot_core::credentials::a_password_in_a_connection_string_is_a_finding`
`verified-by: bravebot_core::credentials::each_shape_matches_at_its_minimum_and_not_below_it`
`verified-by: bravebot_core::credentials::one_key_in_a_json_field_is_one_finding`
`verified-by: bravebot_core::credentials::an_armoured_private_key_is_one_finding_over_its_whole_body`
`verified-by: bravebot_core::credentials::nothing_a_finding_says_repeats_the_value`

<a id="CRED-17"></a>
### CRED-17: no gate passes because the scan ran

A finding informs and decides nothing. It grants nothing, refuses nothing by itself, and moves no
credential between tiers.

**Why.** A program running as the person presents itself as any other, so a scanner running here can
be lied to about what the tree holds. The scan defends against the person not knowing what is in
their own repository, which is worth doing and is not a control.

`verified-by: none`

<a id="CRED-18"></a>
### CRED-18: no part of the scan discloses the tree off this machine

Any classifier over tree content runs locally. Nothing is sent to a hosted model to ask whether it
is a credential, and no credential is exercised against its provider to find out whether it still
works.

**Why.** A scan that ships content to a model discloses the credential to that model and becomes the
leak it was checking for. Testing a key is a use of that key, from this machine, at this time.

`verified-by: none`

<a id="CRED-19"></a>
### CRED-19: a finding carries no credential value, and is written where the planner does not read

A finding records the kind, the location, a salted fingerprint and a masked preview that is not a
prefix or a suffix of the real value. It is written outside the tree, kept out of the prompt, and
shown to the person by a path the planner does not read.

**Why.** A finding that quotes the value has moved a Held credential into a second Held location and
called it a feature. Writing the findings into the tree hands a map of every credential in the
repository to the next thing that reads it.

`verified-by: none`

<a id="CRED-20"></a>
### CRED-20: a disposition moves a tier only by passing the gate it attempts

Removing or denying a path changes no tier. Enrolling reaches Delegated only where a performer
exists for the operation. Accepting changes nothing and owes what Held owes.

**Why.** Denying a path keeps a value out of the context while work proceeds, which is the right
trade and is not custody. A credential enrolled with no performer to use it is a Held credential in
a better cupboard.

`verified-by: none`

<a id="CRED-21"></a>
### CRED-21: accepting a finding expires

An acceptance carries an expiry, and lapses into a finding again when it passes.

**Why.** An acceptance with no end is a finding that was deleted slowly.

`verified-by: none`

<a id="CRED-22"></a>
### CRED-22: the baseline is not writable by a turn

Entries are added by a person, carry a fingerprint and a reason and an expiry rather than a value,
and stop applying when the fingerprint stops matching. They live where CRED-19 puts a finding,
outside the tree, because an allowlist in the tree is the same map CRED-19 refuses to write.

**Why.** An allowlist is necessary or the scan becomes noise everybody clicks through, and it is the
obvious target: a turn that can add to the baseline can clear its own leak. A fingerprint that no
longer matches is a new finding rather than a renewed acceptance.

`verified-by: none`

<a id="CRED-23"></a>
### CRED-23: credential buffers are cleared, and kept out of crash artifacts

A buffer this program owns a credential in is cleared when it is dropped rather than returned to the
allocator intact, and the process excludes credentials from the artifacts a crash leaves behind:
core dumps disabled or the pages excluded from them, and those pages kept off swap where the
platform allows it.

**Why.** The inventory lists core dumps, the swap and hibernation files, and crash reports as places
credentials end up, and every one of them is written by the operating system rather than by anything
here. A rule about which files this program writes does not reach them. Without this the strongest
tier is still recoverable from a machine that panicked once, and the panic is not an attack.

**What this is not.** It is not a claim that no copy of the value exists anywhere in the process.
An allocator, a TLS library, an HTTP header buffer and the compiler itself all make copies this
program does not own and cannot clear, so what is promised is the buffers it does own. Nor does it
defend against a debugger attached to a live process, which is the same account and is answered by
the authority holding the value instead.

**What holds today.** The core dumps, the buffers a `Secret` owns, and the buffers reading a settings
file makes on the way to one. The process lowers its core dump limit to nothing before it reads the
first credential, and every credential the backend configuration resolves is kept in a `Secret`, the
gateway token in a settings file included. Reading that file fills buffers of its own: the text it
was read into, the document the parse made of it, and whatever a stronger layer displaced by
restating a gateway a weaker one had stated. The text is cleared as the parse answers, whatever it
answered, and the document clears itself and what it displaced when it goes. Every reader of a
settings file holds the document rather than a bare parse, the front end's check of a file somebody
chose included, so none of them has a clearing to remember.

**What does not, and why.** Three things.

The pages are not kept off swap. Locking the buffers themselves needs the allocator that hands
them out, which this program does not own, and the process-wide form, `mlockall` with
`MCL_FUTURE`, is worse than the thing it prevents: where the memory lock limit allows it at all,
every later allocation becomes unswappable, so an agent asked to read a large file fails to
allocate rather than being paged out. Doing this properly means an allocator for credential
buffers, which is a decision rather than an omission.

The single-use credentials of an imported subscription are plain strings. They live in
`bravebot-skus`, which [LAYER-1](layering.md#LAYER-1) gives no dependency on any other crate here,
so reaching `Secret` from there is a layering change rather than a line.

What a settings file's `env` block sets is a plain string for the length of the run. `Settings` keeps
what each layer said so that the configuration can be built out of it and `doctor` can report which
file won a name, and a signing key set there is a credential in that map: the `Secret` the
configuration builds out of it is cleared, and the map it was read out of has no `Drop` and is not.
Giving that map one means deciding which names in it are credentials, since the same block carries a
region and a model name, and the answer to what a person may put there is anything.

`verified-by: bravebot_config::lib::scrubbing_overwrites_the_bytes_where_they_lie`
`verified-by: bravebot_config::lib::scrubbing_counts_the_bytes_rather_than_the_characters`
`verified-by: bravebot_config::lib::scrubbing_a_document_reaches_a_token_inside_the_blocks_it_was_written_in`
`verified-by: bravebot_config::settings::the_text_a_layer_was_parsed_from_is_cleared`
`verified-by: bravebot_config::settings::a_merge_keeps_the_entry_a_stronger_layer_displaced`
`verified-by: bravebot_sandbox::crash::disabling_core_dumps_leaves_the_kernel_unable_to_write_one`
`verified-by: bravebot_sandbox::crash::disabling_core_dumps_does_not_lower_the_hard_limit`
`verified-by: by-construction (the buffer a Secret owns is unreachable once the Secret is gone, so what a test can run is the scrub rather than the drop; the drop body is one call to the scrub the two tests above pin and does nothing else)`
`verified-by: by-construction (both entry points that hold a credential, the terminal binary and the graphical front end's transport, call the core dump limit down as their first statement, before the argument vector is read and so before the signing key is unmasked)`
`verified-by: by-construction (a parsed settings document clears itself when it goes, its drop being one call to each of the two scrubs the tests above pin and nothing else; the four readers of a settings file, the layered read, the single-file parse, the managed layer and the front end's check of a chosen file, each hold one of these and so clear what they parsed by going out of scope)`

<a id="CRED-24"></a>
### CRED-24: a credential never travels as a command-line argument

No credential, and nothing derived from one that would authenticate in its place, is passed to a
program as an argument. Where a program must receive one, it arrives by environment, on a file
descriptor, or in a file the program is told to read; where the authority can perform the action,
none of those is needed.

**Why.** A command line is world-readable while the process lives: `/proc/<pid>/cmdline` on Linux,
and `ps` on every platform. The environment is covered by [RUN-12](tools/run.md#RUN-12) because a
person never sees it, and this is the opposite case: an argument is shown at the prompt, and
approving it prevents nothing, because the disclosure is to every other account on the machine
rather than to the program. It needs no attacker and no mistake by the model.

`verified-by: none`

<a id="CRED-25"></a>
### CRED-25: every held credential names what would end it

A credential at Held or Held briefly records its issuer, the surface that revokes it, and anything
minted from it that revocation would not reach. Where a credential leaks, that record is what the
person is given.

**Why.** Held is where most of the inventory sits, and its whole obligation is detection. Detection
that cannot be acted on is a notification. Removing a file, which is the disposition people reach
for, ends this run's custody and nothing else: the credential is still live at the issuer, still in
the history, and still in whatever copied it. The difference between an incident somebody can close
and one they can only observe is whether this was written down before it was needed, and it can only
be written down while the arrangement is still understood.

**What it does not claim.** Recording the surface does not revoke anything, and nothing here reaches
an issuer on a person's behalf.

**Where the person is given it.** Not yet from a leak. CRED-16's scan of what a turn writes
exists and refuses, but it reports the finding and reaches no part of this record: somebody told a
credential would have landed in a file is not thereby told what would end the one they already
hold. CRED-15's scan of the tree before a run does not exist, and nor does the place CRED-19 would
write a finding. What exists is the record and one surface that reads it, `doctor`, which reports
what would end each credential this build holds. A scan reaching it later reads that record rather
than writing a second one.

`verified-by: bravebot_config::lib::a_build_that_cannot_sign_for_itself_holds_no_signing_key_to_account_for`
`verified-by: bravebot_config::lib::an_aws_account_holds_both_arrangements_and_they_end_differently`
`verified-by: bravebot_bedrock::credentials::a_session_token_is_what_says_what_would_end_a_credential`
`verified-by: bravebot_cli::main::every_held_credential_has_its_own_account_of_what_would_end_it`
`verified-by: bravebot_cli::main::what_survives_revoking_is_reported_for_exactly_the_credentials_that_have_one`

## Why the gate safehouse builds is not available here

[safehouse](https://github.com/brave-experiments/safehouse) keeps credentials out of the labelled
world entirely, and it does so through several properties rather than one rule. Some of them hold
here too. The ones that do not are what this file exists for.

| what safehouse relies on | in bravebot |
| --- | --- |
| a label is a frozen pair of enums with no string field, so it cannot carry a secret | **holds, structurally.** `Label` here is the same shape, two enums and no string field ([LABEL-1](labels.md#LABEL-1)), and [CRED-1](#CRED-1) adds that a secret never becomes a labelled value at all |
| call parameters are decided from trusted data, with no dirty context | **holds.** Untrusted content never enters the planner ([LABEL-3](labels.md#LABEL-3)), and a decision may be taken only from trusted content ([LABEL-5](labels.md#LABEL-5)) |
| both model calls are SDK calls, and the key travels in a header the model never sees | **holds.** A request carries the key in an `authorization` header, and the body is the conversation |
| the environment is read in one place, so it is not an exposure surface | **fails.** The signing key is read in one place and withheld from a subprocess, with the exception [CRED-14](#CRED-14) records, but the rest of the person's environment is handed to every program a turn runs, on purpose: [RUN-12](tools/run.md#RUN-12) keeps it because `run aws s3 ls` is an ordinary request |
| data slots hold fetched API responses, so no slot holds a credential | **fails.** The content here is the working tree, which the inventory below shows is itself where `.env`, `*.pem` and `*.tfvars` live |
| every component holding a credential is deterministic operator code, with no model in the loop | **fails for all but one.** This agent's own backend credential is spent by deterministic code, as there; every other credential in the inventory is spent by `run`, executing programs [RUN-10](tools/run.md#RUN-10) says are "not enumerated and not confined" |
| a fetcher takes exactly one credential by name, so the wrong one is a `TypeError` rather than a policy violation | **fails.** `run`'s parameter is a command line, so there is no signature to leave a credential out of, and the programs "cannot be listed in advance" |
| a processor receives an instruction and one slot, and the outgoing request is pinned by test to a fixed set of fields | **fails.** The request is a conversation that accumulates file content and command output, is kept by [sessions.md](sessions.md), and is rewritten by [compaction.md](compaction.md) |

**The failures are one fact stated twice.** safehouse fetches structured data from remote services
and acts on it through typed functions. bravebot reads a local working tree and acts on it by
running arbitrary programs. Where the data is fetched, a slot can be arranged to exclude
credentials; where the data is the person's own tree, the credentials are already inside it. Where
the effect is a typed call, a signature bounds what that call may spend; where the effect is a
program the person's shell would run, [RUN-10](tools/run.md#RUN-10)'s own reason applies, which is
that `git push` needs `~/.ssh`.

**So the gate is not missing here, it is unavailable.** Nothing in this file is a weaker version of
those checks. The preconditions they rest on are absent, and no gate placed inside this process
restores them. What remains is to put the credential outside the process, which is what the tiers
below walk through, strongest first.

<a id="CRED-26"></a>
### CRED-26: where a credential can be bound to its presenter, that is the form asked for

A derived credential is asked for in a sender-constrained form wherever the issuer offers one:
mutual TLS, a proof-of-possession scheme such as DPoP, or a workload identity the issuer checks for
itself. Where no such form is offered, what is held is a bearer secret and the record says so.

**Why.** Scope says what a credential may do and expiry says for how long. Neither says where it may
be done from, so a five-minute bearer token that leaves this machine is five minutes of its entire
authority in the hands of whoever took it. Of the four properties a derivative can carry, this is
the only one that makes a stolen value useless rather than merely short-lived.

**Where it sits.** It moves no tier by itself. An unbound derivative that passes gate 2 is Granted,
and so is a bound one; this is the difference between two credentials standing at the same tier, and
it is recorded like everything else the walk decides.

`verified-by: none`

## Known costs

We accept these deliberately. Do not "fix" one without changing this spec first.

- **Most of what is already on the machine is out of reach.** An OS keychain, a cloud cache, a
  browser profile, a shell history: nothing here touches them. Reading one is ordinary file access,
  and stopping a program the agent started from reading one is confinement.

- **The off-scale case is visible, not bounded.** CRED-5 makes ambient authority declared. There is
  no credential to withhold and no environment to scrub, and confinement is the only fix.

- **The scan misses things, and silence proves nothing.** The rarity layer is advisory, history
  beyond the working tree is off by default, and a credential nobody has a pattern for is a
  credential the scan does not find. A clean result means nothing was matched.

- **The diff scan inherits those blind spots.** CRED-16 catches what a pattern matches. A credential
  no layer recognises is written, not found, and recorded as complete. It fails toward silence in
  the one place this system is accountable for.

- **A turn cannot finish a setup step that needs a literal secret in a config file.** CRED-11 and
  CRED-13 together mean a generated framework key goes to the authority and the tree gets a
  reference. Where a framework cannot read a reference, the turn produces the reference, says what
  it could not do, and the person completes it. This breaks scaffolding written on the assumption
  that generated secrets land in a file.

- **Gate 2 is usually failed by the counterparty.** Most SaaS keys, most package registries and most
  webhook secrets have no derived form to ask for. The tier is honest about it and cannot fix it.

- **Held is where most credentials live today**, and its obligations are the cost of that being
  true rather than a promise that it is rare.

- **A credential written through a reference is not scanned.** The scan reads what a turn wrote in
  its own words. A body that arrived as quarantined content is carried to the file without the
  driver reading a byte of it, and examining one to decide whether to refuse would be a decision
  taken from untrusted content, which [labels.md](labels.md) admits nowhere. So a value copied out
  of a file nobody vouched for reaches the tree through a reference. What would close it is a scan
  whose finding nothing here has to branch on.

- **A file a turn creates is attributed to it whole.** A carried value is told from an authored one
  by what the file at that path already held, and a file that did not exist held nothing. So a turn
  that moves a file already holding a key is refused, where the clause above says that case is
  reported. What would tell the two apart is the scan before the run, which is what records what
  was already there.

- **A fingerprint is salted per run and kept nowhere.** It tells two findings in one run apart and
  says nothing between runs, so an acceptance cannot be carried forward and the baseline has
  nothing to match against. A salt that outlives the run is a file somebody has to keep, and it
  belongs with the store a finding is written to.

  One consequence is worth naming: an inferred finding is re-raised every run, because nothing
  remembers that somebody already said a development password was a development password. A person
  working in a tree that holds one answers for it again each session. The allowlist that would fix
  it needs the durable salt and the store above, so this is the cost of not having them yet rather
  than a separate gap.

- **Only what a turn writes is scanned, never what it reads.** A secret already in the tree reaches
  the planner's context the moment a turn reads the file holding it, and nothing looks at it on the
  way through. The gate here is about what this system *causes*; disclosure of what was already
  there is CRED-15's business, and CRED-15 is unbuilt. The two together are why reading a `.env` is
  currently unexamined in both directions.

- **Half of CRED-14 is pinned and half is argued.** The tests on it are the environment of a program
  this agent starts. That what it holds reaches no session record rests on
  [TRACE-2](trace.md#TRACE-2) admitting no field one could sit in, and that it is shown only as a
  redaction rests on the type it is held in; neither is pinned by a test that scans a record or a
  screen for every secret the process holds, which is what the clause says is owed.

- **Most of this is not implemented.** What runs is the scan of what a turn writes, and one
  performer: a credential a vault obtained itself, and a mail send carried out against it so that
  the asking agent never holds the token. There is no scan of the tree before it is vouched for, no
  store a finding is written to and no baseline over one, and no authority at the tier these clauses
  describe. The rest of every clause here is a target.