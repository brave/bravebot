---
id: CRED
title: Credential protection
status: proposed
governs:
  - crates/core/src/ambient.rs
  - crates/config/src/lib.rs
  - crates/config/src/env_var.rs
  - crates/config/src/provider.rs
  - crates/config/src/settings.rs
  - crates/ui-bridge/src/settings.rs
  - crates/bedrock/src/credentials.rs
  - crates/signing/src/sigv4.rs
  - crates/core/src/credentials.rs
  - crates/agent/src/findings.rs
  - crates/sandbox/src/crash.rs
  - crates/skus/src/device.rs
  - crates/skus/src/profile.rs
  - crates/skus/src/secret.rs
  - crates/skus/src/store.rs
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

**Where it is recorded.** On the record [CRED-25](#CRED-25) uses, which is the one per-credential
record there is, and read by the same surface: `doctor` names the tier under the account of what
would end the credential. The tier is the one entry on that record every credential in it owes
whatever its tier, which is why the record reaches past the two tiers CRED-25 asks an account of.

`verified-by: bravebot_config::lib::every_credential_in_the_record_stands_at_a_tier_the_walk_arrived_at`
`verified-by: bravebot_config::lib::an_imported_subscription_is_a_credential_this_configuration_holds`
`verified-by: bravebot_cli::main::each_tier_a_credential_can_stand_at_is_reported_in_its_own_words`
`verified-by: bravebot_cli::running::doctor_names_the_tier_of_every_credential_it_accounts_for`
`verified-by: bravebot_cli::running::doctor_accounts_for_an_imported_subscription_at_the_tier_its_walk_stopped_at`

<a id="CRED-3"></a>
### CRED-3: a gate fails only for a stated reason, and dropping pays what the gate says it costs

Each drop records which of the gate's conditions failed, and whether the counterparty refused or
nobody attempted it. Those are different answers and they end at the same tier.

**Why.** Without the record a tier is an assertion. A gate the counterparty refused is a fact about
the world and a gate nobody attempted is a decision somebody made, and only the second is ours to
revisit.

**Where it is recorded.** Beside the tier, on the record [CRED-25](#CRED-25) uses and read by the
same surface: a credential's walk is the drops it took, in the order the gates are asked, and
`doctor` prints one line per drop under the account of what would end it. The number of drops is
the tier, since failing a gate drops exactly one and nothing skips one, so a walk and a tier that
disagree are a disagreement rather than a silence.

`verified-by: bravebot_config::lib::a_credentials_walk_holds_one_drop_per_gate_it_failed_and_stops_at_its_tier`
`verified-by: bravebot_config::lib::every_drop_this_configuration_records_is_one_nobody_attempted`
`verified-by: bravebot_cli::main::a_reason_is_reported_for_exactly_the_drops_the_record_holds`
`verified-by: bravebot_cli::main::a_drop_is_reported_with_the_gate_and_the_answer_the_record_holds`
`verified-by: bravebot_cli::main::every_drop_has_its_own_account_of_the_condition_it_failed`
`verified-by: bravebot_cli::running::doctor_accounts_for_every_drop_of_each_credentials_walk`

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

`verified-by: bravebot_core::ambient::a_line_that_reaches_a_container_daemon_names_it`
`verified-by: bravebot_core::ambient::a_tool_that_spends_on_one_command_is_named_for_that_command_alone`
`verified-by: bravebot_core::ambient::the_metadata_service_is_named_by_the_address_and_not_by_the_argument`
`verified-by: bravebot_tui::confirm::a_run_prompt_names_the_ambient_authority_a_line_reaches`
`verified-by: bravebot_tui::confirm::a_fetch_prompt_says_what_the_metadata_service_is`
`verified-by: bravebot_agent::turn::spending_an_ambient_authority_is_recorded_in_the_trail_and_an_ordinary_line_is_not`

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

**What this leaves the AWS session credential at.** Held. The AWS CLI is asked for credentials as
each request is built, and what renews one is an export that asks nobody: the next session comes
from whatever the profile chains to, for as long as that lasts. How long that is is a question
about `~/.aws/config`, which nothing here reads, so a `role_arn` over a long-lived key in a file
and an SSO chain arrive as one arrangement, the CLI answering an export. Its walk drops at gate 3
on the agent renewing it without further authority, and [CRED-3](#CRED-3) records that drop as one
nobody attempted: reading the profile chain, and finding one whose renewal a person has to sit
through, would be a different arrangement. So the stated expiry ends that copy rather than this
program's access, which is what the record and the report say.

`verified-by: bravebot_config::lib::no_credential_this_program_holds_stands_at_held_briefly`
`verified-by: bravebot_bedrock::credentials::a_session_this_program_re_mints_unaided_is_recorded_at_held`

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

**Where it is recorded.** Beside the tier, on the same record [CRED-25](#CRED-25) uses, and read by
the same surface: `doctor` prints the figure under the account of what would end the credential. The
tier and the figure are separate answers on that record, so a credential standing at Held briefly
with nothing sized is a disagreement rather than a silence.

**What stands there, and what the figure survives.** Nothing this program holds stands at Held
briefly: the one arrangement whose bound reads as a window is an AWS session credential, and
[CRED-9](#CRED-9) puts it at Held. Its figure stays, because the obligations ratchet downward and
a drop sheds none of them, and because detection is the whole of what a permanent credential is
owed. What the drop changes is what the figure decides. At Held briefly it says whether the window
is short enough for what the credential reaches; at Held there is no window, and it says how long
a leak runs before anybody goes to the surface [CRED-25](#CRED-25) names. So a credential at Held
briefly has a figure and one at Held may, which is one direction rather than two.

`verified-by: bravebot_config::lib::a_credential_at_held_briefly_is_sized_against_detection`
`verified-by: bravebot_cli::main::how_soon_a_leak_is_noticed_is_reported_for_exactly_the_credentials_the_record_sizes`

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

**A line a turn runs is one of the ways it writes.** The check does not belong to the write tools:
a command line reaches a file through a redirection, and the line itself is a place, since it is
drawn on a screen, kept in the record of the round and readable by every account on the machine
while the program lives. So the line is scanned as the turn's own words before anything is
compiled or shown, and what the line left at each destination it opened is scanned once it has
stopped, at the label that destination's own effect is recorded under. The second of those two
cannot refuse before the fact, because the program opens the file itself: what it buys is the
value being taken back out of the tree rather than never reaching it, and the costs below say
what that is worth and where it stops.

`verified-by: bravebot_agent::turn::a_credential_the_line_itself_carries_stops_the_line`
`verified-by: bravebot_agent::turn::a_credential_a_run_line_redirects_into_the_tree_does_not_stay_there`
`verified-by: bravebot_agent::turn::a_credential_the_destination_already_held_does_not_refuse_the_line_carrying_it`

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

**What the refusal says.** A credential created as a file is the value and nothing else, so there
is no room in it for the reference CRED-11's refusal asks for, and a planner told to write one
there writes it into the file a framework reads as the key. What that turn is told instead is that
nothing was created, that generating another and writing it elsewhere is the same refusal again,
and that the person is the one who creates the value. Telling the two apart is a question about the
body rather than about the finding: a value inside a document was copied from wherever it already
sat, and a value that is the whole file had no prior location.

`verified-by: bravebot_agent::turn::a_credential_created_as_a_whole_file_is_not_created_and_the_planner_is_told_so`
`verified-by: bravebot_core::credentials::a_generated_key_standing_as_a_whole_file_is_recognised`
`verified-by: bravebot_core::credentials::a_file_that_is_the_value_is_told_from_one_that_mentions_it`
`verified-by: bravebot_core::credentials::blank_lines_around_the_value_are_not_contents`

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
### CRED-15: what a turn reads is scanned before the planner receives it

The text a read would hand the planner is scanned first, with the layers CRED-16 uses and under
CRED-18 and CRED-19. On a finding the result is held back and the person is asked whether to go
ahead, told that the read would expose a credential to the model. The question names the path and
the finding, which is a kind, a location and a masked preview, and never the value. On yes the
result reaches the planner as it would have; on no the planner is told the read was declined
because the file holds what looks like a credential, and gets none of its text. The finding goes to
the person and never to the planner, whichever way it was answered, which is what CRED-19 requires
of a finding however it arose.

**Why the ordering is the whole design.** The planner's context goes to whoever performs inference,
so a file handed to it has been handed to them. A scan that ran afterwards would be reporting a
disclosure that had already happened, which is why this runs at the read rather than at the end of
the turn or at the moment somebody vouches for the tree.

**Only trusted text is scanned.** A read of content nobody vouched for hands the planner a
reference rather than bytes, so there is nothing disclosed to hold back, and reading those bytes to
decide whether to ask would be a decision taken from untrusted content, which
[labels.md](labels.md) admits nowhere. It is the same bound a credential written through a
reference falls under, and the Known costs record both.

**What the question covers, and for how long.** The file, for the session. An answer writes no
rule, moves no credential between tiers and grants nothing a gate reads: the read was already
allowed by the trust map before the scan ran, which is what CRED-17 means by a finding deciding
nothing. Remembering it per path is what stops a planner reading the same `.env` on round after
round putting the same question up each time. It does not outlive the session, and the Known costs
say what that rests on.

**Why the person is asked rather than the read refused.** The rarity layer is a guess, and a tree
holds development passwords, test fixtures and inline manifests as readily as it holds keys. A gate
that refused every such read with no way to say otherwise would make an ordinary `.env` unreadable
to a planner working on one, and a scan people have to fight is a scan they turn off. The person
owns the tree and is the one who can say which it is.

`verified-by: bravebot_agent::turn::a_read_that_would_expose_a_credential_is_held_back_until_the_person_agrees`
`verified-by: bravebot_agent::turn::what_a_read_would_expose_is_told_to_the_person_and_never_to_the_planner`
`verified-by: bravebot_agent::turn::the_read_prompt_says_which_value_it_is_asking_about`
`verified-by: bravebot_agent::turn::a_file_agreed_to_once_is_not_asked_about_again_this_session`
`verified-by: bravebot_agent::turn::a_file_agreed_to_in_an_earlier_turn_is_not_asked_about_again`
`verified-by: bravebot_agent::turn::a_read_of_a_file_nobody_vouched_for_is_not_scanned`
`verified-by: bravebot_agent::turn::a_read_of_a_file_holding_no_credential_is_not_asked_about`
`verified-by: bravebot_agent::turn::a_credential_in_history_is_held_back_until_the_person_agrees`

<a id="CRED-16"></a>
### CRED-16: what a turn writes to the tree is scanned before the change is recorded as complete

The tree diff a turn produces is scanned with the same layers and the same finding shape as the
read scan, under CRED-18 and CRED-19. A finding in that diff is attributed to the turn, is a
violation of CRED-11 or CRED-13, and the value does not remain in the tree.

**Why.** CRED-15 scans what leaves the tree for a model, so without this the one thing this system
causes is the only thing never checked. The two are the two directions a credential travels: that
one is about disclosure of what was already there, and this one is the difference between a
credential the person left and a credential a turn wrote.

**Why attribution is better here than before.** An untracked `.env` found at the start has no blame
and no turn record, so `unknown` is a real answer. A diff names the turn that produced it, which is
why this scan can refuse where the other can only inform. It is not authorship: a turn that
reformats or moves a file already holding a key produces a diff carrying it without having written
it, and that case is reported rather than refused. A whole-file write uses that attribution only
when the pre-image was trusted at capture, whether the write came from a turn or from a manifest.
Untrusted prior bytes cannot excuse a finding in a proposed trusted body.

**What is refused and what is asked about.** A value that declared itself a credential (a
provider's prefix over its own alphabet at its own length, or the password field of a URL) is
refused, and nobody is asked: there is no judgement to put to anybody, and a prompt that can be
answered "write it anyway" is a prompt a turn eventually gets past. A value inferred from a name
that sounds like a secret beside one that looks rare is raised on the approval the write already
needs, and the person decides. That inference catches a generated framework key, and it also
catches an inline Kubernetes `Secret`, a local development password and a test fixture; refusing on
all four with no override would stop ordinary work over a guess, and a scan people have to fight is
a scan they turn off. The prompt names the finding, so the question can be answered.

A value standing as the whole of a file, with no name beside it and no provider prefix on it, is
inferred the same way and raised the same way. The file being nothing else is what stands in for
the name, which is weaker than a name: a commit id, a machine identifier and a digest are written
in that shape too.

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
`verified-by: bravebot_core::credentials::a_provider_key_standing_alone_is_one_finding_and_not_two`
`verified-by: bravebot_core::credentials::a_rare_token_with_a_document_around_it_is_not_a_whole_file`
`verified-by: bravebot_core::credentials::a_value_standing_as_a_whole_file_is_a_question_and_not_a_rule`
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

**Where it is written.** One file per workspace under the state directory, beside the record of
granted rules and keyed the same way, holding one entry per finding: the kind, the path, the line,
the fingerprint and the preview. A finding is written whatever was done about it, because what was
refused, what was approved and what the person declined are equally things in their tree. The
screen alone is not enough: a line drawn while nobody was looking is gone when the turn ends, and
the scan exists to tell a person what their own repository holds.

The record is best effort and fails toward silence. No state directory, a full disk, a read-only
home: each means the findings do not outlive the session, which is where they were before the
record existed, and none of them refuses a write the person approved. An
[incognito](incognito.md#INCOG-5) session writes none, and reads the ones an earlier ordinary
session left.

`verified-by: bravebot_agent::findings::a_finding_one_session_recorded_is_read_back_by_another`
`verified-by: bravebot_agent::findings::nothing_in_the_record_repeats_the_value`
`verified-by: bravebot_agent::findings::a_finding_made_in_one_workspace_is_not_read_back_in_another`
`verified-by: bravebot_agent::findings::a_workspace_sharing_a_key_with_another_does_not_read_its_findings`
`verified-by: bravebot_agent::findings::a_second_finding_is_added_rather_than_replacing_the_first`
`verified-by: bravebot_agent::findings::a_record_that_cannot_be_read_holds_nothing`
`verified-by: bravebot_agent::findings::a_line_nothing_can_read_leaves_the_rest_of_the_record_readable`
`verified-by: bravebot_agent::findings::an_entry_this_build_does_not_fully_understand_is_not_read`
`verified-by: bravebot_agent::findings::a_scan_that_found_nothing_leaves_no_record`
`verified-by: bravebot_agent::findings::a_turn_with_no_session_to_name_still_records_what_it_found`
`verified-by: bravebot_agent::turn::a_finding_is_written_outside_the_tree_and_outlives_the_turn`
`verified-by: bravebot_agent::turn::a_credential_a_line_left_at_a_destination_is_written_to_the_record`
`verified-by: bravebot_agent::turn::a_credential_in_the_line_itself_is_written_to_the_record`
`verified-by: bravebot_agent::turn::what_the_scan_found_is_told_to_the_person_and_not_to_the_planner`
`verified-by: bravebot_agent::incognito::no_credential_finding_is_written_down`
`verified-by: bravebot_agent::incognito::a_finding_an_earlier_session_recorded_is_still_read`
`verified-by: bravebot_core::credentials::nothing_a_finding_says_repeats_the_value`
`verified-by: bravebot_core::credentials::a_preview_is_no_part_of_the_value_it_describes`

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
outside the tree, because an allowlist in the tree is the same map CRED-19 refuses to write. That
record exists; what an entry cannot yet be matched on is a fingerprint, which is salted per run.

**Why.** An allowlist is necessary or the scan becomes noise everybody clicks through, and it is the
obvious target: a turn that can add to the baseline can clear its own leak. A fingerprint that no
longer matches is a new finding rather than a renewed acceptance.

`verified-by: none`

<a id="CRED-23"></a>
### CRED-23: credential buffers are cleared, and kept out of crash artifacts

A buffer this program owns a credential in is cleared when it is dropped rather than returned to the
allocator intact, and the process excludes credentials from the artifacts a crash leaves behind:
core dumps disabled or the pages excluded from them, and the pages `bravebot-config`'s `Secret`
holds a credential in kept off swap by a Unix kernel that grants the lock.

**Why.** The inventory lists core dumps, the swap and hibernation files, and crash reports as places
credentials end up, and every one of them is written by the operating system rather than by anything
here. A rule about which files this program writes does not reach them. Without this the strongest
tier is still recoverable from a machine that panicked once, and the panic is not an attack.

**What this is not.** It is not a claim that no copy of the value exists anywhere in the process.
An allocator, a TLS library, an HTTP header buffer and the compiler itself all make copies this
program does not own and cannot clear, so what is promised is the buffers it does own. Nor does it
defend against a debugger attached to a live process, which is the same account and is answered by
the authority holding the value instead.

**What holds today.** The core dumps, the pages a `Secret` is held in, the buffers reading a
settings file makes on the way to one, and the buffers an imported subscription's credentials pass
through.
The process lowers its core dump limit to nothing before it reads the first credential, and every
credential the backend configuration resolves is kept in a `Secret`, the gateway token in a settings
file included. Reading that file fills buffers of its own: the text it was read into, the document
the parse made of it, and whatever a stronger layer displaced by restating a gateway a weaker one
had stated. The text is cleared as the parse answers, whatever it answered, and the document clears
itself and what it displaced when it goes. Every reader of a settings file holds the document rather
than a bare parse, the front end's check of a file somebody chose included, so none of them has a
clearing to remember. The single-use credentials of an imported subscription are in a `Secret` too,
and so is every buffer they pass through on the way there: the batch a session spends from, the text
of the file it was read out of and the document that parse made, the document a write serialises,
the reply a registration decodes its signed tokens from, and the browser preferences file an import
reads an order id out of, which holds that install's own credentials. `bravebot-skus` has its own,
because [LAYER-1](layering.md#LAYER-1) gives that crate no dependency on another crate here; what
has to hold across the two is that the bytes are overwritten where they lie and that reading them is
visible at the call site. What a settings file's `env` block set is cleared too: the settings keep
what each layer said so the configuration can be built out of it and `doctor` can report which file
won a name, and a signing key written there is a credential in that map, so the map is overwritten
when the settings go. Every value in it rather than the names a credential is known to arrive under,
which is the reading a parsed document already gets and is here for the same reason: the same block
carries a region and a model name, and what a person may put in it is anything. Nothing holds a
`Settings` for the length of the run, which would be a map that never goes: the one place that used
to, the list of variables a subprocess is not handed, keeps the names it read out of one rather than
the settings themselves, and a name is not a credential.

A `Secret` holds its value in pages of its own, locked out of swap. `bravebot-config` forbids
`unsafe`, so the mapping and the lock are `bravebot-sandbox`'s: the value is copied into an
anonymous mapping nothing else is placed in, locked before the copy is written, overwritten where it
lies when it goes, and unmapped, which releases the lock with the pages. A mapping per value rather
than a lock on the heap, because a lock is on a page and a heap page is shared: locking a value in
place would lock its neighbours, and unlocking it would unlock a second credential that happened to
share the page. The string a `Secret` is made from is overwritten once the value is copied out of
it, so the credential does not stay on the heap it was moved off, and a clone is a second mapping
locked the same way.

The AWS credential the CLI resolves is held the same way, and so is every buffer it passes through.
The bytes the CLI wrote are cleared as the read is answered for, whatever it answered, because a
reply that could not be parsed holds the credential just as a good one does. The document that read
parsed holds a copy of its own and clears itself when it goes. A reply stating a secret key and no
access key is refused, and the secret it did state is in a `Secret` by then rather than in a string
the refusal drops intact. Signing makes one more: the SigV4 derivation starts from the secret access
key with four bytes in front of it, which is the key rather than something derived from it, so that
buffer is overwritten where it lies once the first step has read it. `bravebot-signing` spells the
overwriting out itself, for the reason `bravebot-skus` does, since
[LAYER-1](layering.md#LAYER-1) gives it no dependency on another crate here.

**What does not, and why.** Three things.

Only a `Secret`'s pages are kept off swap. What holds a credential on its way into one is on the
ordinary heap: the text of a settings file and the document parsed from it, the bytes the AWS CLI
replied with, and the SigV4 seed. Each is overwritten once it has been read, and locking it would
take an allocator for everything the parse allocates, which this program does not own. A
credential exported into the environment stays in the process's environment block, which the C
library owns and nothing here overwrites. The process-wide form, `mlockall` with `MCL_FUTURE`, is
worse than the thing it prevents: where the memory lock limit allows it at all, every later
allocation becomes unswappable, so an agent asked to read a large file fails to allocate rather than
being paged out. `bravebot-skus` keeps its own `Secret` on the heap as well:
[LAYER-1](layering.md#LAYER-1) gives that crate no dependency to take a lock through, and a Leo
batch of 576 credentials, read and dropped together, would want one mapping for the batch rather
than a page for each.

Where no lock is granted the value is held anyway, and nothing reports it. `RLIMIT_MEMLOCK` is one
limit for every `Secret` the process holds at once, each rounded up to whole pages, and an import
holds the settings file it rewrites twice: as read, at most sixteen pages of four kilobytes, and as
rewritten, whose size is checked only once it is held. A value held once the limit is spent is held
in an unlocked mapping. A Windows build holds it on the heap, since `VirtualLock` is not
among the bindings this build compiles and no suite runs there to check one.
Refusing to hold a credential would protect it by making the product unusable. A hibernation image
is written from resident memory and includes locked pages, so what the lock keeps out of swap it
does not keep out of that file.

What the cryptography turns a credential into is not cleared. The token values the library hands
back from unblinding a subscription credential are its buffers rather than this program's, and the
presentation a spend derives is an ordinary string: single-use, bound to one issuer, built to be a
request header, and copied from there into the buffers the paragraph above says are not this
program's either. A SigV4 signing key is the same case: the chain authorises one service in one
region on one day, so what each step hands back is not the access key it started from.

`verified-by: bravebot_config::lib::scrubbing_overwrites_the_bytes_where_they_lie`
`verified-by: bravebot_config::lib::scrubbing_counts_the_bytes_rather_than_the_characters`
`verified-by: bravebot_config::lib::scrubbing_a_document_reaches_a_token_inside_the_blocks_it_was_written_in`
`verified-by: bravebot_config::lib::the_string_a_secret_is_made_from_is_overwritten_once_it_is_held`
`verified-by: bravebot_sandbox::swap::held_text_is_in_pages_the_kernel_keeps_resident`
`verified-by: bravebot_sandbox::swap::a_copy_is_held_in_locked_pages_of_its_own`
`verified-by: bravebot_sandbox::swap::a_value_that_goes_gives_its_locked_pages_back`
`verified-by: bravebot_sandbox::swap::a_refused_lock_still_holds_the_value_and_says_so`
`verified-by: bravebot_sandbox::swap::clearing_overwrites_the_bytes_where_they_lie`
`verified-by: bravebot_config::settings::the_text_a_layer_was_parsed_from_is_cleared`
`verified-by: bravebot_config::settings::a_merge_keeps_the_entry_a_stronger_layer_displaced`
`verified-by: bravebot_config::settings::what_the_env_block_was_set_to_is_overwritten_where_it_lies`
`verified-by: bravebot_config::scrub::the_names_a_settings_file_added_are_what_is_kept_rather_than_the_settings`
`verified-by: bravebot_skus::secret::scrubbing_overwrites_the_bytes_where_they_lie`
`verified-by: bravebot_skus::secret::scrubbing_counts_the_bytes_rather_than_the_characters`
`verified-by: bravebot_skus::secret::scrubbing_a_document_reaches_a_token_inside_the_blocks_it_was_written_in`
`verified-by: bravebot_skus::secret::a_secret_prints_as_redacted_rather_than_as_its_value`
`verified-by: bravebot_skus::store::a_stored_batch_does_not_print_its_tokens`
`verified-by: bravebot_bedrock::credentials::the_bytes_a_reply_was_read_from_are_cleared_whatever_the_read_answered`
`verified-by: bravebot_bedrock::credentials::scrubbing_a_parsed_reply_overwrites_the_credential_rather_than_dropping_it`
`verified-by: bravebot_signing::sigv4::scrubbing_the_signing_seed_overwrites_the_key_where_it_lies`
`verified-by: bravebot_sandbox::crash::disabling_core_dumps_leaves_the_kernel_unable_to_write_one`
`verified-by: bravebot_sandbox::crash::disabling_core_dumps_does_not_lower_the_hard_limit`
`verified-by: by-construction (the pages a Secret holds its value in are unmapped once it is gone, so what a test can run is the clearing rather than the drop; the drop body is one call to the clearing the test above pins and then the unmapping, after which the test above finds the lock gone, and does nothing else)`
`verified-by: by-construction (a Secret is one field, the held text, made only by the hold the test above pins, so its value is reachable only in that text's pages, which the tests above find locked where the kernel grants the lock, and its clone is that text's clone)`
`verified-by: by-construction (the text a subscription batch is read from, the document it is parsed into and the document it is written back as are each held in a guard for the whole of the call that makes one, so every way out clears it; each guard's drop body is one call to a scrub the tests above pin and does nothing else)`
`verified-by: by-construction (both entry points that hold a credential, the terminal binary and the graphical front end's transport, call the core dump limit down as their first statement, before the argument vector is read and so before the signing key is unmasked)`
`verified-by: by-construction (a parsed settings document clears itself when it goes, its drop being one call to each of the two scrubs the tests above pin and nothing else; the four readers of a settings file, the layered read, the single-file parse, the managed layer and the front end's check of a chosen file, each hold one of these and so clear what they parsed by going out of scope)`
`verified-by: by-construction (the env block a Settings holds is unreachable once the Settings is gone, so what a test can run is the overwriting rather than the drop; the drop body is one call to the method the test above pins and does nothing else)`
`verified-by: by-construction (the document a credential reply was parsed into and the buffer a SigV4 signing key is seeded from are each unreachable once the value owning them is gone, so what a test can run is the overwriting rather than the drop; each drop body is one call to a scrub the tests above pin and does nothing else)`

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
hold. CRED-15's scan of what a turn reads reports the same way and reaches this record no more
than the other one does. What exists is the record and one surface that reads it, `doctor`, which
reports
what would end each credential this configuration holds, a gateway's bearer token and an imported
subscription's credential batch included. A scan reaching it later reads that record rather than
writing a second one.

`verified-by: bravebot_config::lib::a_build_that_cannot_sign_for_itself_holds_no_signing_key_to_account_for`
`verified-by: bravebot_config::lib::an_aws_account_holds_both_arrangements_and_they_end_differently`
`verified-by: bravebot_config::lib::a_gateway_token_is_a_credential_this_configuration_holds`
`verified-by: bravebot_config::provider::the_host_a_token_would_be_ended_at_carries_no_other_part_of_the_endpoint`
`verified-by: bravebot_bedrock::credentials::a_session_token_is_what_says_what_would_end_a_credential`
`verified-by: bravebot_cli::main::every_held_credential_has_its_own_account_of_what_would_end_it`
`verified-by: bravebot_cli::main::a_gateway_token_is_accounted_for_at_the_gateway_that_would_end_it`
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

- **What names an ambient authority is a list, and a list has an end.** A container daemon, a
  logged-in tool, the agent socket and a metadata address are recognised by the words a line
  writes, so a client nobody listed is granted with only the blanket line about confinement said
  of it. Nothing is refused on the list, so what a gap costs is a sentence rather than a boundary,
  and the direction to be wrong in is naming something a line was not going to spend.

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

- **There is nothing to create a credential into, so creation is refused rather than performed.**
  CRED-13 asks for the value to reach an authority in the same step and its tier to be recorded
  then. No authority exists, so what runs is the clause's other half: the creation does not happen
  and the turn says so. Nothing records a tier for a credential a turn created, because there is no
  such credential to record one for.

- **A credential created as a file is inferred, so the refusal can be overridden.** The shape is a
  file whose whole contents is one rare value, which is what says it is a value rather than a
  document. A commit id, a machine identifier and a digest are written that way too, so the finding
  goes to the person on the approval the write already needs rather than refusing outright, and a
  person who says yes gets the write. Where the value also declares itself under CRED-16, it is
  refused with nobody asked, as any declared value is. What would close the gap is an authority to
  create into, which is the entry above.

- **A value a command generates gets less than one a write tool creates.** CRED-16 reads what a
  turn wrote in its own words, and a redirection is a file a program the turn started opened for
  itself: `openssl rand -hex 32 > config/master.key` puts the value in the tree without a byte of
  it passing through a layer on the way. What the line left at that destination is read back
  afterwards, under the two bounds the entries below record: nothing is read where the line's own
  output is content nobody vouched for, and a value only the third layer recognises has no prompt
  to go to at this tool. So a generated key written this way is reported to the person where it is
  seen at all, and a person deciding about it is what is missing.

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

- **What a line left is scanned only where somebody vouched for the line.** The same cost as the
  entry above, reached by the other route. A program may print anything, so what a line prints is
  content nobody vouched for unless every step of it was vouched for in this tree or is an audited
  call over paths the person opened; below that, the destination is carried to the filesystem and
  not read. So a turn copying `.env` to `.env.bak` before editing it is refused in a tree where
  the person has vouched for the line and not in one where they have not. Closing it needs the
  same thing the entry above does.

- **A path a program writes without the line naming it is not scanned at all.** The destinations
  a line opens are the ones a redirection names, which is the set the driver reserves as file
  effects before anything runs. `cp .env .env.bak`, `tee`, `git commit` and an installer writing
  its own config are outside it: the path is the program's business and the driver never learns
  it. What would close this is a scan of the whole tree diff a turn produced, which needs a record
  of what the tree held before it, which is CRED-22's baseline.

- **A destination past the scan's budget is passed over.** What a line left is read back for the
  scan, and what it held beforehand is kept so a refused destination can be put back; both are
  bounded, and past the bound neither is done. A finding that could not be acted on would stop a
  line and leave the value where it landed, which is worse than saying nothing, so nothing is
  said. A turn that wanted to get past this would arrange for the destination to be large first.

- **A line gets the refusal and not the question.** A value that declared itself is refused at the
  run tool as it is at a write tool, but the inferred layer's finding has nowhere to be put to
  anybody: the run approval carries no finding, and by the time the destination can be read the
  line has already run. So a name that sounds like a secret beside a value that looks rare is
  reported to the person afterwards rather than decided by them beforehand, which is a notice and
  not a judgement. Building the other half means the run prompt carrying what a write prompt
  already carries, and it can only cover a value the line itself holds.

- **A file a turn creates is attributed to it whole.** A carried value is told from an authored one
  by what the file at that path already held, and a file that did not exist held nothing. So a turn
  that moves a file already holding a key is refused, where the clause above says that case is
  reported. What would tell the two apart is a record of what the tree held before the run, which
  is CRED-22's baseline.

- **A fingerprint is salted per run and kept nowhere.** It tells two findings in one run apart and
  says nothing between runs, so an acceptance cannot be carried forward and the baseline has
  nothing to match against. A salt that outlives the run is a file somebody has to keep. The store
  CRED-19 writes to now exists and holds the fingerprint each run made, so what a durable salt is
  still waiting on is the decision to keep one rather than somewhere to keep it.

  One consequence is worth naming: an inferred finding is re-raised every run, because nothing
  remembers that somebody already said a development password was a development password. A person
  working in a tree that holds one answers for it again each session. The allowlist that would fix
  it needs the durable salt and the store above, so this is the cost of not having them yet rather
  than a separate gap.

- **`read_file` is the only read that is scanned.** CRED-15 runs at the tool whose whole purpose
  is putting a file's text in front of the planner. Three other results carry a vouched file's
  bytes there and are not scanned: `search` quotes the lines it matched, `load_skill` carries the
  body of a skill, and `read_output` hands over what a program printed. Each needs its own answer
  rather than the same one. A search walks many files at once and mixes vouched ones with
  quarantined ones, so a question about it is a question about a set rather than a path, and the
  finding's location is the thing a person would act on. A skill is a file the person installed
  under their own state directory rather than something the tree proposed. What a program printed
  is quarantined unless the line was vouched for, and where it was not there is a prompt in front
  of it already. Until those are settled a credential in a file reaches the planner through any of
  the three, and the disclosure is the same one CRED-15 is about.

- **The desktop application declines rather than asking.** Its protocol has no question of this
  shape and its front end draws no screen for one, so a file the scan objects to is held back from
  the planner there whatever the person would have said. It is the same answer that application
  gives a fetch, a language server and a manifest plan, and it is the safe direction: what it costs
  is the text of one file, and the planner is told why it did not get it.

- **An answer lasts the session and no longer.** Agreeing that a file may be read is remembered per
  path while the session lives and is written nowhere, so the next session asks about the same
  `.env` again. Carrying it forward needs a name an answer can be filed under across runs, which is
  the durable salt and the store the entry above describes: a path alone is not one, since the file
  at that path is not the file that was answered about.

- **Half of CRED-14 is pinned and half is argued.** The tests on it are the environment of a program
  this agent starts. That what it holds reaches no session record rests on
  [TRACE-2](trace.md#TRACE-2) admitting no field one could sit in, and that it is shown only as a
  redaction rests on the type it is held in; neither is pinned by a test that scans a record or a
  screen for every secret the process holds, which is what the clause says is owed.

- **Most of this is not implemented.** What runs is the scan of what a turn writes, at the three
  write tools and at the two places a `run` line writes, and one performer: a credential a vault
  obtained itself, and a mail send carried out against it so that the asking agent never holds the
  token, and the record CRED-19 writes each finding to. The scan of what a turn reads runs at
  `read_file` and nowhere else. There is no baseline over that record, and no authority at the tier
  these clauses describe. The rest of every clause here is a target.
