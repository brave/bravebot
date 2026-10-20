## Lane: what a cloned repository's files can set

A repository arrives with files the person did not write: the project and local settings files under
`.bravebot/`, `AGENTS.md`, skill and agent definitions, a server list, and anything else the program
reads from the working directory. Settings load before the trust question, and answering no does not
change what was loaded. Whoever wrote the repository wrote those files, and they reach the same code
the person's own files reach.

The preamble's sentence that the rule says nothing about "this program's own configuration and
startup" covers the person's own configuration: their user settings file, their command line and
their environment. A checkout's files are written by somebody else and read before anyone is asked
anything, so that sentence is no reason to drop a candidate about them. This lane asks one question:
**what can a cloned repository's files set, and does any of it decide a destination, a credential, a
permission, a program that starts or a string that is shown?**

`BACKEND-24` says which layer answers for which name, and `BACKEND-33` names the variables an `env`
block sets and the six read from the person's layers alone:

{layer_clauses}

`PINNABLE` in `crates/config/src/managed.rs` lists the names a machine's managed layer may pin, which
its comment describes as the ones that decide where a request goes. A name on it is one a checkout
could otherwise decide.

### Where to start

Where the layers are merged, where an unread key and the text of a permission rule are taken from a
file, and where the names a file set are listed:

{layered_sites}

The fields of `Settings`, which are what a settings file can end up setting: {settings_fields}

### What to ask

For each field, and for each name in the `env` table:

**Which layers can set it.** Follow the field from `layered_since` to the merge rule BACKEND-24 states
for it. A name that resolves to the closest layer can be set by a project or local file.

**What it decides.** A host, a region, a proxy, a certificate bundle, a profile, a model, or a path
the program reads, writes or starts. A value that is later put into a URL, a header, a command line, a
sandbox row or a file name is a decision. Check what validates it on the way. A value that is checked
where one name uses it and unchecked where a neighbouring name uses it is the shape to look for: a
project or location refused when it holds a slash, an at sign, a question mark or a hash, beside a
region or a profile that is not.

**What is signed, sent or granted to it.** Where a checkout's value moves a request, ask what the
request carries: a credential's signature, a header, the conversation. Where it decides a permission,
ask whether it can widen one. A list that can only narrow, which BACKEND-24 gives `scrubEnv` and the
permission lists as, is sound. A value that selects or replaces is not.

**What it makes the program print.** A key nothing reads, an entry that does not parse, an `env` name
and a path in a sandbox list are each printed back by `doctor`, `-p` and `--plain`, in some cases
before the trust question. Trace each string from the file to the writer and report one that reaches
a terminal with its control characters.

**Whether the file is the person's.** A file the command line names is the person's act, and it is
read after the others only when it sits outside the checkout. `layered_since` takes the directory the
session started in as well as the current one. Check that a named file inside the checkout is treated
as the checkout's.

### What is not this lane

The person's own settings file, or a variable the person exported. A name BACKEND-24 drops from a
project layer and reports. A project value that selects among options the person has already
approved. And a finding whose whole content is that a repository can write a file: name the field, the
layer that reads it and the decision it takes, or do not file it.
