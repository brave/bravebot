# Words a person reads

Every one of them lives in `crates/i18n/locales/`, one file per locale, and reaches the screen
through `t!(some_message)`. `en-US.ftl` is the reference: it owns the set of messages and the name
and kind of every argument, so a translation can add none of its own and break no call site.

```sh
make locales              # what each translation has of the reference, and what it is missing
make check-locales        # hold the catalogs to the record of what they are missing
make write-untranslated   # rewrite that record after translating something
```

Adding a language is copying `en-US.ftl` and translating it. No Rust changes, no registration:
the build script finds the file. See
[crates/i18n/locales/README.md](../../crates/i18n/locales/README.md), which is written for whoever
is doing the translating rather than for whoever wrote this.

Adding a **message** means adding it to `en-US.ftl` first, because the macro has one arm per
message in the reference and a name no catalog defines does not compile. The other catalogs can
follow later; what they lack is shown in English.

Later, not never. A gap has to be recorded in
[untranslated-messages.txt](../../untranslated-messages.txt), and `make check-locales` fails while
that file and the catalogs disagree, in either direction: a message a catalog is missing that the
file does not list, and a line for a gap that is no longer there. So translate it in the same
commit, or run `make write-untranslated` and commit what it writes. The reason it is a file rather
than a warning is that a warning is what this used to be, and eleven messages shipped untranslated
behind it: the build only prints one when the build script actually runs, and a passing job's log
is not read.

The distinction that matters here is the audience, not the crate. The words the planner reads are
not in a catalog and must not be: a tool's description, the preamble, and the sentence a refused
tool answers with are interface to a model, and rewording them in another language changes what
the agent does. `crates/agent/tests/audience.rs` fails if a catalog lookup appears in one of those
modules. [specs/localization.md](../specs/localization.md) is the spec, and its known costs list what
is deliberately left in English.
