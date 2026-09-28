//! Importing a model service Claude Code or opencode configured (`docs/specs/import.md`).
//!
//! What the two programs' files hold is read and mapped in [`bravebot_config::import`]. Here is
//! the rest: what is shown, the questions, and the write. Every question is put in lines, the way a
//! session in lines puts one, because each is asked before anything is drawn.

use crate::exit::{Ending, fail};
use crate::plain::Prompting;
use crate::progress::printable;
use bravebot_agent::confirm::Decision;
use bravebot_config::bedrock::{Bedrock, Tier};
use bravebot_config::import::{self, Destination, Found, Gateway, Key, Left, Reason, Source};
use bravebot_config::provider::{Credential, Provider};
use bravebot_config::{Config, Managed};
use bravebot_i18n::t;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// What the sources hold, for a refusal that asks nothing.
#[derive(Debug, Default)]
pub(crate) struct Looked {
    /// The lines naming what was found and not imported.
    left: Vec<String>,
    /// The sources holding something the import command would offer.
    importable: Vec<Source>,
}

impl Looked {
    /// The lines naming what was found and not imported.
    pub(crate) fn left(&self) -> &[String] {
        &self.left
    }

    /// The line naming the command that would ask, where there is anything to ask about.
    pub(crate) fn command(&self) -> Option<String> {
        match self.importable.as_slice() {
            [] => None,
            [source] => Some(t!(onboarding_import_one, source = source.name())),
            [first, second, ..] => Some(t!(
                onboarding_import_both,
                first = first.name(),
                second = second.name()
            )),
        }
    }
}

/// Where a start would be refused for naming no model service, offer the import first, and end the
/// start unless a service answers after it. Both front ends open their session through this.
pub(crate) fn before_the_session(config: &mut Config) -> Option<ExitCode> {
    let bravebot_agent::backend::Serving::NothingConfigured {
        subscription,
        a_service_is_configured,
    } = bravebot_agent::backend::serving(
        config,
        &bravebot_net::Egress::new(),
        &crate::model_for_this_run(None, config),
    )
    else {
        return None;
    };
    match at_the_start(a_service_is_configured) {
        Start::Serving(imported) => {
            *config = *imported;
            None
        }
        Start::Ended(code) => Some(code),
        Start::Refuse(looked) => Some(fail(
            Ending::Configuration,
            crate::how_to_configure_a_model(
                subscription.as_deref(),
                a_service_is_configured,
                &looked,
            ),
        )),
    }
}

/// How a start with nothing configured goes on, once the import has been offered.
enum Start {
    /// Nothing was imported: the start refuses as BACKEND-39 says, with these lines in it.
    Refuse(Looked),
    /// Something was imported and a service answers now, on this configuration read back from disk.
    Serving(Box<Config>),
    /// The start ends here, having said why.
    Ended(ExitCode),
}

/// Offer the import at a start that BACKEND-39 would refuse.
///
/// Asked only where there is somebody to answer and somewhere to write: all three streams are
/// terminals, since the questions go to stderr, the session is not incognito, and no service is
/// configured, which is the three-route case. Anywhere else the refusal gains a line naming the
/// command that asks, and nothing is read from stdin.
fn at_the_start(a_service_is_configured: bool) -> Start {
    let asks = !a_service_is_configured
        && !bravebot_core::incognito::engaged()
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
        && std::io::stderr().is_terminal();
    if !asks {
        return Start::Refuse(looked(a_service_is_configured));
    }
    let Some(file) = destination_file() else {
        return Start::Refuse(Looked::default());
    };
    let found = import::found(&import::Places::from_env(), exported);
    if found.is_empty() {
        return Start::Refuse(Looked::default());
    }

    let mut destination = match Destination::open(&file) {
        Ok(destination) => destination,
        Err(_) => return Start::Refuse(looked(false)),
    };
    // The process's own stdin, whose buffer is shared with every later reader of it, so an answer
    // typed ahead here reaches whatever asks next rather than a buffer that is dropped.
    let mut asking = Prompting::new(std::io::stdin().lock(), std::io::stderr());
    let offered = offer(
        &mut asking,
        found,
        &mut destination,
        &Managed::load(),
        &exported,
    );
    if offered.imported.is_empty() {
        return Start::Refuse(Looked::default());
    }
    if let Err(problem) = write(&destination) {
        return Start::Ended(fail(Ending::Failed, problem));
    }
    for source in &offered.imported {
        asking.say(&imported(*source, &file));
    }

    // Read again as a fresh start reads it, since the file on disk is what every later start will
    // read and a write that did not make a working configuration is worth finding now.
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(err) => {
            return Start::Ended(fail(
                Ending::Configuration,
                t!(cli_configuration_problem, problem = err),
            ));
        }
    };
    // Only the session's own model has to answer before it opens. Another entry's key can be
    // exported later, and ending here over it would refuse a service that works.
    let unset = unset_gateways(&config, &offered.gateways);
    let blocked = config
        .provider_for(&crate::model_for_this_run(None, &config))
        .is_some_and(|(serving, _)| unset.iter().any(|provider| provider.id == serving.id));
    if blocked {
        let lines: Vec<String> = unset
            .iter()
            .map(|provider| unset_line(provider, &file, false))
            .collect();
        return Start::Ended(fail(Ending::Configuration, lines.join("\n")));
    }
    for provider in &unset {
        asking.say(&unset_line(provider, &file, true));
    }
    match bravebot_agent::backend::serving(
        &config,
        &bravebot_net::Egress::new(),
        &crate::model_for_this_run(None, &config),
    ) {
        bravebot_agent::backend::Serving::NothingConfigured {
            subscription,
            a_service_is_configured,
        } => Start::Ended(fail(
            Ending::Configuration,
            crate::how_to_configure_a_model(
                subscription.as_deref(),
                a_service_is_configured,
                &Looked::default(),
            ),
        )),
        _ => Start::Serving(Box::new(config)),
    }
}

/// What the sources hold, where nobody is asked.
///
/// Nothing where a service is configured, which is the one-line case, and nothing while incognito,
/// which writes nothing and so has nothing to offer.
pub(crate) fn looked(a_service_is_configured: bool) -> Looked {
    let mut looked = Looked::default();
    if a_service_is_configured {
        return looked;
    }
    let Some(file) = destination_file() else {
        return looked;
    };
    let destination = Destination::open(&file);
    let managed = Managed::load();
    for found in import::found(&import::Places::from_env(), exported) {
        let plan = plan(found, destination.as_ref().ok(), &managed, &exported);
        if plan.adds_anything() {
            looked.importable.push(plan.source);
        }
        looked.left.extend(left_lines(plan.source, &plan.left));
    }
    // The command would refuse on this same file, so the file is what is named.
    if let Err(why) = destination
        && !looked.importable.is_empty()
    {
        looked.importable.clear();
        looked.left.push(unwritable(why, &file));
    }
    looked
}

/// `bravebot import-providers`: the same questions, at any time.
pub(crate) fn providers(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        return fail(Ending::Argument, t!(import_takes_nothing_else));
    }
    // Before the terminal, as `import-leo-creds` refuses: this is a write, and the one thing an
    // incognito session will not do.
    if bravebot_core::incognito::engaged() {
        return fail(Ending::Failed, t!(import_not_while_incognito));
    }
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return fail(Ending::Argument, t!(import_needs_a_terminal));
    }
    let Some(file) = destination_file() else {
        return fail(Ending::Failed, t!(import_no_home));
    };
    let mut destination = match Destination::open(&file) {
        Ok(destination) => destination,
        Err(why) => return fail(Ending::Failed, unwritable(why, &file)),
    };

    let found = import::found(&import::Places::from_env(), exported);
    let mut asking = Prompting::new(std::io::stdin().lock(), std::io::stderr());
    let offered = offer(
        &mut asking,
        found,
        &mut destination,
        &Managed::load(),
        &exported,
    );
    if !offered.asked {
        asking.say(match offered.settled {
            true => t!(import_nothing_new),
            false => t!(import_nothing_found),
        });
        return ExitCode::SUCCESS;
    }
    if offered.imported.is_empty() {
        return ExitCode::SUCCESS;
    }
    if let Err(problem) = write(&destination) {
        return fail(Ending::Failed, problem);
    }
    for source in &offered.imported {
        asking.say(&imported(*source, &file));
    }
    // Said rather than failed on: a service may already be configured beside this one, and the
    // import did what it was asked.
    if let Ok(config) = Config::from_env() {
        for provider in unset_gateways(&config, &offered.gateways) {
            asking.say(&unset_line(provider, &file, false));
        }
    }
    ExitCode::SUCCESS
}

fn exported(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// The user's own settings file, or `None` where nothing may be written.
fn destination_file() -> Option<PathBuf> {
    bravebot_agent::home::writable()
        .map(|directory| bravebot_config::user_settings_file(&directory))
}

/// What the questions came to.
#[derive(Debug, Default)]
struct Offered {
    /// Whether anything was asked at all.
    asked: bool,
    /// Whether a source held something importable that the file already sets or the managed layer
    /// pins, so that nothing was asked about it.
    settled: bool,
    /// The sources somebody said yes to, in the order they were asked about.
    imported: Vec<Source>,
    /// The ids of the gateways written.
    gateways: Vec<String>,
}

/// Ask about each source in turn, and add what is approved to `destination`.
///
/// Planned against the destination as the answers before left it, so a name the first answer wrote
/// is one the second finds already set.
fn offer<R: BufRead, W: Write>(
    asking: &mut Prompting<R, W>,
    found: Vec<Found>,
    destination: &mut Destination,
    managed: &Managed,
    environment: &dyn Fn(&str) -> Option<String>,
) -> Offered {
    let mut offered = Offered::default();
    for found in found {
        let plan = plan(found, Some(destination), managed, environment);
        if !plan.adds_anything() {
            offered.settled |=
                !plan.kept.is_empty() || !plan.pinned.is_empty() || !plan.named.is_empty();
            let lines = [
                kept_lines(&plan, destination.path()),
                named_lines(&plan),
                pinned_lines(&plan, managed),
                left_lines(plan.source, &plan.left),
            ];
            for line in lines.iter().flatten() {
                asking.say(line);
            }
            continue;
        }
        offered.asked = true;
        let lines = shown(&plan, destination.path(), managed);
        let question = t!(import_question, source = plan.source.name());
        if asking.ask(&lines, &question) != Decision::Approve {
            continue;
        }
        offered.imported.push(plan.source);
        let Plan {
            env,
            model,
            gateways,
            ..
        } = plan;
        for (name, value) in env {
            destination.add_env(&name, &value);
        }
        if let Some(model) = model {
            destination.add_model(&model);
        }
        for mut gateway in gateways {
            credential(asking, &mut gateway, destination.path());
            offered.gateways.push(gateway.id.clone());
            destination.add_gateway(gateway);
        }
    }
    offered
}

/// Settle what the written entry says about its credential.
///
/// A key the source holds is written only on a question of its own, which names where it goes and
/// never shows it. Anything else leaves the entry naming the variable to export, or nothing where no
/// variable is known, which is how a local model server is configured.
fn credential<R: BufRead, W: Write>(
    asking: &mut Prompting<R, W>,
    gateway: &mut Gateway,
    file: &Path,
) {
    let kept = match std::mem::replace(&mut gateway.key, Key::Named) {
        Key::Named => return,
        Key::Held(key) => {
            let question = t!(
                import_key_question,
                id = printable(&gateway.id),
                endpoint = printable(&gateway.endpoint),
                file = printable(&file.display().to_string())
            );
            let approved = asking.ask(&[], &question) == Decision::Approve;
            if approved {
                gateway.keep_key(&key);
            }
            approved
        }
        Key::File(_) => false,
    };
    if kept {
        return;
    }
    if let Some(fallback) = gateway.fallback() {
        gateway.name_variable(fallback);
    }
    let variables = gateway.variables();
    asking.say(&match variables.is_empty() {
        true => t!(import_key_none, id = printable(&gateway.id)),
        false => t!(
            import_key_export,
            id = printable(&gateway.id),
            variables = printable(&variables.join(", "))
        ),
    });
}

/// What one source would add, against what the file and the managed layer already say.
struct Plan {
    source: Source,
    read: Vec<PathBuf>,
    env: Vec<(String, String)>,
    model: Option<String>,
    gateways: Vec<Gateway>,
    /// Names the file already sets, which keep their values.
    kept: Vec<String>,
    /// Models left out of an AWS entry because a tier names them, by entry id.
    named: Vec<(String, String, Tier)>,
    /// Names the managed layer pins, which it would override.
    pinned: Vec<String>,
    left: Vec<Left>,
}

impl Plan {
    fn adds_anything(&self) -> bool {
        !self.env.is_empty() || self.model.is_some() || !self.gateways.is_empty()
    }
}

/// `destination` is `None` where the file could not be read as a document, which is said when the
/// import is asked for rather than here.
fn plan(
    found: Found,
    destination: Option<&Destination>,
    managed: &Managed,
    environment: &dyn Fn(&str) -> Option<String>,
) -> Plan {
    let mut plan = Plan {
        source: found.source,
        read: found.read,
        env: Vec::new(),
        model: None,
        gateways: Vec::new(),
        kept: Vec::new(),
        named: Vec::new(),
        pinned: Vec::new(),
        left: found.left,
    };
    let holds = |check: &dyn Fn(&Destination) -> bool| destination.is_some_and(check);

    for (name, value) in found.env {
        if managed.get(&name).is_some() {
            plan.pinned.push(format!("env.{name}"));
        } else if holds(&|file| file.holds_env(&name)) {
            plan.kept.push(format!("env.{name}"));
        } else {
            plan.env.push((name, value));
        }
    }
    // The tiers as a start will read them once this plan is written: pinned, then exported, then
    // the file.
    let tiers = Bedrock::from_lookup(|name| {
        managed
            .get(name)
            .map(str::to_string)
            .or_else(|| environment(name).filter(|value| !value.trim().is_empty()))
            .or_else(|| {
                plan.env
                    .iter()
                    .find(|(set, _)| set == name)
                    .map(|(_, value)| value.clone())
            })
            .or_else(|| {
                destination
                    .and_then(|file| file.env(name))
                    .map(str::to_string)
            })
    });
    // A managed `provider` block is the whole list of gateways on this machine, so one added here
    // would never be read.
    let gateways_pinned = managed.gateways().is_some();
    for mut gateway in found.gateways {
        let name = format!("provider.{}", gateway.id);
        if gateways_pinned {
            plan.pinned.push(name);
        } else if holds(&|file| file.holds_gateway(&gateway.id)) {
            plan.kept.push(name);
        } else {
            let taken = tiers
                .as_ref()
                .map(|tiers| gateway.take_tier_models(tiers))
                .unwrap_or_default();
            // An AWS entry offers only the models it names, so one left with none adds nothing.
            let emptied = !taken.is_empty() && !gateway.names_models();
            plan.named.extend(
                taken
                    .into_iter()
                    .map(|(model, tier)| (gateway.id.clone(), model, tier)),
            );
            if !emptied {
                plan.gateways.push(gateway);
            }
        }
    }
    // An opencode model answers through the entry written beside it, and through no entry this
    // file or the managed layer already has.
    let served = found
        .model_gateway
        .is_none_or(|id| plan.gateways.iter().any(|gateway| gateway.id == id));
    if let Some(model) = found.model.filter(|_| served) {
        match holds(&|file| file.holds_model()) {
            true => plan.kept.push("model".to_string()),
            false => plan.model = Some(model),
        }
    }
    plan
}

/// Everything that is shown before a source's question: the file written, every name added with
/// its value as written, and the names left out and why.
///
/// Every value goes through [`printable`], because a file somebody else's program wrote can hold
/// an escape sequence, and one here would redraw the lines around the question.
fn shown(plan: &Plan, destination: &Path, managed: &Managed) -> Vec<String> {
    let file = printable(&destination.display().to_string());
    let read = plan
        .read
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let found = match plan.read.is_empty() {
        true => t!(import_found_exported, source = plan.source.name()),
        false => t!(
            import_found,
            source = plan.source.name(),
            files = printable(&read)
        ),
    };
    let mut lines = vec![found, t!(import_adds, file = file.as_str())];
    for (name, value) in &plan.env {
        lines.push(printable(&format!(
            "  env.{name}: {}",
            import::quoted(value)
        )));
    }
    if let Some(model) = &plan.model {
        lines.push(printable(&format!("  model: {}", import::quoted(model))));
    }
    for gateway in &plan.gateways {
        lines.push(format!(
            "  {}",
            t!(
                import_adds_gateway,
                id = printable(&gateway.id),
                endpoint = printable(&gateway.endpoint),
                entry = printable(&gateway.shown())
            )
        ));
        match &gateway.key {
            Key::Named => {}
            Key::Held(_) => lines.push(format!(
                "  {}",
                t!(import_key_held, id = printable(&gateway.id))
            )),
            Key::File(path) => lines.push(format!(
                "  {}",
                t!(
                    import_key_file,
                    id = printable(&gateway.id),
                    path = printable(path)
                )
            )),
        }
    }
    lines.extend(kept_lines(plan, destination));
    lines.extend(named_lines(plan));
    lines.extend(pinned_lines(plan, managed));
    lines.extend(left_lines(plan.source, &plan.left));
    lines
}

/// The models a tier already names, which are not added to an entry a second time.
fn named_lines(plan: &Plan) -> Vec<String> {
    if plan.named.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![t!(import_named).to_string()];
    lines.extend(plan.named.iter().map(|(id, model, tier)| {
        format!(
            "  {}",
            t!(
                import_named_model,
                id = printable(id),
                model = printable(&import::quoted(model)),
                variable = tier.env_var()
            )
        )
    }));
    lines
}

/// The names the file already sets, which keep their values.
fn kept_lines(plan: &Plan, file: &Path) -> Vec<String> {
    if plan.kept.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![t!(
        import_kept,
        file = printable(&file.display().to_string())
    )];
    lines.extend(
        plan.kept
            .iter()
            .map(|name| format!("  {}", printable(name))),
    );
    lines
}

/// The names the managed layer pins, under the file that pins them.
fn pinned_lines(plan: &Plan, managed: &Managed) -> Vec<String> {
    if plan.pinned.is_empty() {
        return Vec::new();
    }
    let pinning = managed
        .path()
        .map(Path::to_path_buf)
        .unwrap_or_else(bravebot_config::managed_file);
    let mut lines = vec![t!(
        import_pinned,
        file = printable(&pinning.display().to_string())
    )];
    lines.extend(
        plan.pinned
            .iter()
            .map(|name| format!("  {}", printable(name))),
    );
    lines
}

/// One line per thing found and left, by name and reason and never by value.
fn left_lines(source: Source, left: &[Left]) -> Vec<String> {
    if left.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![t!(import_left_heading, source = source.name())];
    for Left { name, reason } in left {
        let why = match reason {
            Reason::AnthropicApi => t!(import_left_anthropic_api),
            Reason::Vertex => t!(import_left_vertex),
            Reason::BearerToken => t!(import_left_bearer_token),
            Reason::NoRegion => t!(import_left_no_region),
            Reason::SignIn => t!(import_left_sign_in),
            Reason::AnotherSdk => t!(import_left_another_sdk),
            Reason::NoEndpoint => t!(import_left_no_endpoint),
            Reason::Substitution => t!(import_left_substitution),
        };
        lines.push(format!("  - {}: {why}", printable(name)));
    }
    lines
}

fn imported(source: Source, file: &Path) -> String {
    t!(
        import_imported,
        source = source.name(),
        file = printable(&file.display().to_string())
    )
}

fn unwritable(why: import::Unwritable, file: &Path) -> String {
    let file = printable(&file.display().to_string());
    match why {
        import::Unwritable::NotADocument => t!(import_not_a_document, file = file),
        import::Unwritable::TooLarge => t!(import_too_large, file = file),
        import::Unwritable::Changed => t!(import_changed, file = file),
    }
}

/// Every written gateway whose variables are all unset here.
fn unset_gateways<'a>(config: &'a Config, written: &[String]) -> Vec<&'a Provider> {
    config
        .providers
        .iter()
        .filter(|provider| written.contains(&provider.id))
        .filter(|provider| {
            matches!(
                provider.credential(|name| std::env::var(name).ok()),
                Credential::Absent
            )
        })
        .collect()
}

/// The line naming what `provider` reads its key from, which is unset. `later` where the session
/// opens anyway, on a model another entry serves.
fn unset_line(provider: &Provider, file: &Path, later: bool) -> String {
    let id = printable(&provider.id);
    let variables = printable(&provider.env.join(", "));
    let file = printable(&file.display().to_string());
    match later {
        false => t!(
            import_unset_variable,
            id = id,
            variables = variables,
            file = file
        ),
        true => t!(
            import_unset_variable_later,
            id = id,
            variables = variables,
            file = file
        ),
    }
}

/// Put the destination on disk, whole, in place of the file it was read from.
///
/// Written beside it under a name of its own at STATE-1's mode, then renamed over it, so a failed
/// write leaves the old file as it was and no copy of a key is readable by another user in between.
/// Through a link rather than over it, so a settings file kept among somebody's dotfiles stays
/// where they keep it.
fn write(destination: &Destination) -> Result<(), String> {
    let path = destination.path();
    let said = |problem: String| {
        t!(
            import_not_written,
            file = printable(&path.display().to_string()),
            problem = problem
        )
    };
    // The questions take as long as the person does, and another program may write the file
    // meanwhile.
    if destination.changed() {
        return Err(unwritable(import::Unwritable::Changed, path));
    }
    let text = destination
        .text()
        .map_err(|why| unwritable(why, destination.path()))?;
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let Some(parent) = target.parent() else {
        return Err(said(target.display().to_string()));
    };
    bravebot_agent::home::create_directory(parent).map_err(|err| said(err.to_string()))?;
    let temporary = parent.join(format!(".settings.json.import-{}", std::process::id()));
    let written = bravebot_agent::home::write_file(&temporary, text.expose().as_bytes())
        .and_then(|()| std::fs::rename(&temporary, &target));
    if let Err(err) = written {
        let _ = std::fs::remove_file(&temporary);
        return Err(said(err.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_config::import::Places;

    /// A scratch profile directory that removes itself.
    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/test-scratch")
                .join(name);
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create scratch");
            Self { path }
        }

        fn write(&self, relative: &str, text: &str) -> PathBuf {
            let path = self.path.join(relative);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("create parent");
            std::fs::write(&path, text).expect("write");
            path
        }

        fn settings(&self) -> PathBuf {
            self.path.join(".bravebot").join("settings.json")
        }

        /// What the two programs' files under this home hold, with nothing exported.
        fn found(&self) -> Vec<Found> {
            let home = self.path.display().to_string();
            let lookup = |name: &str| (name == "HOME").then(|| home.clone());
            import::found(&Places::from_lookup(lookup), lookup)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    const BEDROCK: &str = r#"{"env": {
        "CLAUDE_CODE_USE_BEDROCK": "1",
        "AWS_REGION": "us-west-2",
        "ANTHROPIC_DEFAULT_OPUS_MODEL": "arn:aws:bedrock:us-west-2:1:application-inference-profile/opus"
    }, "model": "opus"}"#;

    const OPENROUTER_HELD: &str = r#"{"provider": {"openrouter": {
        "options": {"apiKey": "sk-or-a-held-key"},
        "models": {"z-ai/glm-4.6": {}}
    }}, "model": "openrouter/z-ai/glm-4.6"}"#;

    /// Run the questions over `answers` against this home's settings file, returning what was said
    /// and what they came to.
    fn asked(scratch: &Scratch, answers: &str, managed: &Managed) -> (String, Offered) {
        asked_about(scratch, scratch.found(), answers, managed, &|_| None)
    }

    /// [`asked`], about what `found` holds, with `environment` exported.
    fn asked_about(
        scratch: &Scratch,
        found: Vec<Found>,
        answers: &str,
        managed: &Managed,
        environment: &dyn Fn(&str) -> Option<String>,
    ) -> (String, Offered) {
        let mut said = Vec::new();
        let offered = {
            let mut asking = Prompting::new(answers.as_bytes(), &mut said);
            let mut destination = Destination::open(&scratch.settings()).expect("a document");
            let offered = offer(&mut asking, found, &mut destination, managed, environment);
            if !offered.imported.is_empty() {
                write(&destination).expect("written");
            }
            offered
        };
        (String::from_utf8(said).expect("text"), offered)
    }

    /// The settings file with its whitespace taken out, which no value these tests write holds.
    fn written(scratch: &Scratch) -> String {
        let text = std::fs::read_to_string(scratch.settings()).expect("the file was written");
        text.split_whitespace().collect()
    }

    /// IMPORT-5: the host is where a credential will be sent, so it and every other name written
    /// are on the screen before the question, value for value.
    #[test]
    fn every_host_and_name_written_is_shown_before_the_question() {
        let scratch = Scratch::new("cli-import-shown");
        scratch.write(".claude/settings.json", BEDROCK);
        scratch.write(
            ".config/opencode/opencode.json",
            r#"{"provider": {
                "gw": {"env": ["GW_TOKEN"], "options": {"baseURL": "https://gw.example.invalid/v1"}},
                "openrouter": {"env": ["OPENROUTER_API_KEY"]}
            }}"#,
        );

        let (said, _) = asked(&scratch, "", &Managed::default());

        let claude = said
            .find("Import this from Claude Code?")
            .expect("Claude Code was asked about");
        let opencode = said
            .find("Import this from opencode?")
            .expect("opencode was asked about");
        let before_claude = &said[..claude];
        for shown in [
            r#"env.BRAVEBOT_USE_BEDROCK: "1""#,
            r#"env.AWS_REGION: "us-west-2""#,
            r#"env.ANTHROPIC_DEFAULT_OPUS_MODEL: "arn:aws:bedrock:us-west-2:1:application-inference-profile/opus""#,
            r#"model: "opus""#,
            &scratch.settings().display().to_string(),
        ] {
            assert!(
                before_claude.contains(shown),
                "{shown} was not shown before the question: {said}"
            );
        }
        let before_opencode = &said[claude..opencode];
        // The compiled-in host for a known id is in no field of the entry, so only the line naming
        // where it is reached shows it.
        for shown in [
            "provider.gw",
            "https://gw.example.invalid/v1",
            r#""env":["GW_TOKEN"]"#,
            "provider.openrouter",
            "https://openrouter.ai/api/v1",
        ] {
            assert!(
                before_opencode.contains(shown),
                "{shown} was not shown before the question: {said}"
            );
        }
    }

    /// IMPORT-5: a setup exported rather than written down was read from no file, and the line
    /// above the question says where it was found instead of naming none.
    #[test]
    fn a_setup_found_only_in_the_environment_says_so() {
        let scratch = Scratch::new("cli-import-exported");
        let home = scratch.path.display().to_string();
        let lookup = |name: &str| match name {
            "HOME" => Some(home.clone()),
            "CLAUDE_CODE_USE_BEDROCK" => Some("1".to_string()),
            "AWS_REGION" => Some("us-west-2".to_string()),
            _ => None,
        };
        let found = import::found(&Places::from_lookup(lookup), lookup);

        let (said, _) = asked_about(&scratch, found, "", &Managed::default(), &lookup);

        let question = said
            .find("Import this from Claude Code?")
            .expect("Claude Code was asked about");
        assert!(
            said[..question]
                .contains("Claude Code configures a model service bravebot can use, in this process's environment."),
            "{said}"
        );
        assert!(!said.contains(", in ."), "{said}");
    }

    /// IMPORT-5: only the affirmative approves; another word declines, and so does nobody
    /// answering at all.
    #[test]
    fn only_the_affirmative_writes_and_the_end_of_input_declines() {
        let scratch = Scratch::new("cli-import-affirmative");
        scratch.write(".claude/settings.json", BEDROCK);

        for answers in ["", "yes please\n", "n\n", "\n"] {
            let (said, offered) = asked(&scratch, answers, &Managed::default());
            assert!(offered.asked, "{answers:?}: nothing was asked: {said}");
            assert!(
                offered.imported.is_empty() && !scratch.settings().exists(),
                "{answers:?} wrote the file"
            );
        }

        let (_, offered) = asked(&scratch, "y\n", &Managed::default());
        assert_eq!(offered.imported, [Source::ClaudeCode]);
        let file = written(&scratch);
        assert!(file.contains(r#""BRAVEBOT_USE_BEDROCK":"1""#), "{file}");
    }

    /// IMPORT-5: two sources that disagree are two questions, and the second finds what the first
    /// answer wrote already set.
    #[test]
    fn a_name_the_first_answer_wrote_is_left_by_the_second() {
        let scratch = Scratch::new("cli-import-second");
        scratch.write(".claude/settings.json", BEDROCK);
        scratch.write(
            ".config/opencode/opencode.json",
            r#"{"model": "openrouter/z-ai/glm-4.6", "provider": {"openrouter": {"env": ["OPENROUTER_API_KEY"]}}}"#,
        );

        let (said, offered) = asked(&scratch, "y\ny\n", &Managed::default());

        assert_eq!(offered.imported, [Source::ClaudeCode, Source::Opencode]);
        let second = &said[said.find("Import this from Claude Code?").expect("asked")..];
        let kept = second.find("Left as they are").expect("something was left");
        assert!(
            second[kept..].contains("  model"),
            "the model the first answer wrote was offered again: {said}"
        );
        let file = written(&scratch);
        assert!(file.contains(r#""model":"opus""#), "{file}");
        assert!(file.contains(r#""openrouter":{"#), "{file}");
    }

    const OPUS: &str = "arn:aws:bedrock:us-west-2:1:application-inference-profile/opus";
    const SOL: &str = "arn:aws:bedrock:us-west-2:1:application-inference-profile/sol";

    /// An opencode AWS entry in us-west-2 naming `models`.
    fn aws_entry(models: &[&str]) -> String {
        let models: Vec<String> = models.iter().map(|id| format!(r#""{id}": {{}}"#)).collect();
        format!(
            r#"{{"provider": {{"amazon-bedrock": {{"options": {{"region": "us-west-2"}}, "models": {{{}}}}}}}}}"#,
            models.join(", ")
        )
    }

    /// IMPORT-5: the tiers answer for a model before any entry does, so a model the first answer's
    /// tier names is not written into the second's AWS entry as well, and an entry left naming
    /// nothing is not offered.
    #[test]
    fn a_model_a_tier_names_is_not_added_to_an_aws_entry_again() {
        let scratch = Scratch::new("cli-import-tier-named");
        scratch.write(".claude/settings.json", BEDROCK);
        scratch.write(".config/opencode/opencode.json", &aws_entry(&[OPUS, SOL]));

        let (said, offered) = asked(&scratch, "y\ny\n", &Managed::default());

        assert_eq!(offered.imported, [Source::ClaudeCode, Source::Opencode]);
        let second = &said[said.find("Import this from Claude Code?").expect("asked")..];
        let entry = second
            .lines()
            .find(|line| line.contains("provider.amazon-bedrock, reached at"))
            .expect("the entry was shown");
        assert!(entry.contains(SOL) && !entry.contains(OPUS), "{said}");
        assert!(
            second.contains(&format!(
                r#""{OPUS}" in provider.amazon-bedrock, named by ANTHROPIC_DEFAULT_OPUS_MODEL"#
            )),
            "{said}"
        );
        let file = written(&scratch);
        assert_eq!(file.matches(OPUS).count(), 1, "{file}");
        assert!(file.contains(SOL), "{file}");

        // Declined, no tier names it, so the entry keeps it.
        let _ = std::fs::remove_file(scratch.settings());
        let (said, _) = asked(&scratch, "n\ny\n", &Managed::default());
        assert!(!said.contains("Not added"), "{said}");
        let file = written(&scratch);
        assert!(file.contains(OPUS) && file.contains(SOL), "{file}");

        let _ = std::fs::remove_file(scratch.settings());
        scratch.write(".config/opencode/opencode.json", &aws_entry(&[OPUS]));
        let (said, offered) = asked(&scratch, "y\ny\n", &Managed::default());
        assert_eq!(offered.imported, [Source::ClaudeCode]);
        assert!(!said.contains("Import this from opencode?"), "{said}");
        assert!(
            said.contains("named by ANTHROPIC_DEFAULT_OPUS_MODEL"),
            "{said}"
        );
        assert!(!written(&scratch).contains("provider"), "{said}");
    }

    /// IMPORT-5: an exported tier is read at run time as a written one is, so it names the model
    /// just the same.
    #[test]
    fn an_exported_tier_names_a_model_as_a_written_one_does() {
        let scratch = Scratch::new("cli-import-tier-exported");
        scratch.write(".config/opencode/opencode.json", &aws_entry(&[OPUS, SOL]));
        let home = scratch.path.display().to_string();
        let lookup = |name: &str| match name {
            "HOME" => Some(home.clone()),
            "BRAVEBOT_USE_BEDROCK" => Some("1".to_string()),
            "AWS_REGION" => Some("us-west-2".to_string()),
            "ANTHROPIC_DEFAULT_OPUS_MODEL" => Some(OPUS.to_string()),
            _ => None,
        };
        let found = import::found(&Places::from_lookup(lookup), lookup);

        let (said, _) = asked_about(&scratch, found, "y\n", &Managed::default(), &lookup);

        assert!(
            said.contains("named by ANTHROPIC_DEFAULT_OPUS_MODEL"),
            "{said}"
        );
        let file = written(&scratch);
        assert!(!file.contains(OPUS) && file.contains(SOL), "{file}");
    }

    /// IMPORT-6: a key the source holds is its own question, which names where it goes and says it
    /// is kept in plain text, and the key itself is never on the screen.
    #[test]
    fn a_held_key_is_asked_about_separately_and_never_drawn() {
        let scratch = Scratch::new("cli-import-held");
        scratch.write(".config/opencode/opencode.json", OPENROUTER_HELD);

        let (said, _) = asked(&scratch, "y\ny\n", &Managed::default());

        let source = said.find("Import this from opencode?").expect("asked");
        let key = said.find("in plain text").expect("the key was asked about");
        assert!(source < key, "the key was asked about first: {said}");
        let question = &said[source..];
        for named in [
            "provider.openrouter",
            "https://openrouter.ai/api/v1",
            &scratch.settings().display().to_string(),
        ] {
            assert!(
                question.contains(named),
                "{named} is not in the question: {said}"
            );
        }
        assert!(
            !said.contains("sk-or-a-held-key"),
            "the key was drawn: {said}"
        );
        let file = written(&scratch);
        assert!(file.contains(r#""apiKey":"sk-or-a-held-key""#), "{file}");
    }

    /// IMPORT-6: declined, the key is not written, and the entry names the variable opencode reads
    /// for that id instead, with a line saying to export it.
    #[test]
    fn a_declined_key_names_the_variable_to_export() {
        let scratch = Scratch::new("cli-import-declined");
        scratch.write(".config/opencode/opencode.json", OPENROUTER_HELD);

        let (said, _) = asked(&scratch, "y\nn\n", &Managed::default());

        let file = written(&scratch);
        assert!(file.contains(r#""env":["OPENROUTER_API_KEY"]"#), "{file}");
        assert!(
            !file.contains("sk-or-a-held-key"),
            "a declined key was written"
        );
        let told = said
            .find("OPENROUTER_API_KEY: export it")
            .expect("told to export");
        assert!(told > said.find("in plain text").expect("asked"), "{said}");

        // With no variable known for the id, the entry is written with no credential, and says so.
        scratch.write(
            ".config/opencode/opencode.json",
            r#"{"provider": {"local": {"options": {"baseURL": "http://127.0.0.1:11434/v1", "apiKey": "unused"}}}}"#,
        );
        let _ = std::fs::remove_file(scratch.settings());
        let (said, _) = asked(&scratch, "y\n\n", &Managed::default());
        assert!(said.contains("no credential"), "{said}");
        assert_eq!(
            written(&scratch),
            r#"{"provider":{"local":{"options":{"baseURL":"http://127.0.0.1:11434/v1"}}}}"#
        );
    }

    /// IMPORT-7: the import adds and replaces nothing, and a name already set is shown as left.
    #[test]
    fn a_name_already_set_keeps_its_value() {
        let scratch = Scratch::new("cli-import-kept");
        scratch.write(".claude/settings.json", BEDROCK);
        scratch.write(
            ".bravebot/settings.json",
            r#"{"env": {"AWS_REGION": "eu-central-1"}, "theme": "light"}"#,
        );

        let (said, _) = asked(&scratch, "y\n", &Managed::default());

        let kept = said
            .find("Left as they are")
            .expect("the kept name was shown");
        assert!(said[kept..].contains("env.AWS_REGION"), "{said}");
        assert!(!said.contains(r#"env.AWS_REGION: "us-west-2""#), "{said}");
        let file = written(&scratch);
        for kept in [
            r#""AWS_REGION":"eu-central-1""#,
            r#""theme":"light""#,
            r#""BRAVEBOT_USE_BEDROCK":"1""#,
        ] {
            assert!(file.contains(kept), "{kept} is not in {file}");
        }
    }

    /// IMPORT-7: the questions take as long as the person does, and writing the document read
    /// before them over a file changed since would lose the change.
    #[test]
    fn a_file_changed_while_asking_is_not_written_over() {
        let scratch = Scratch::new("cli-import-changed");
        scratch.write(".claude/settings.json", BEDROCK);
        let mut said = Vec::new();
        let mut asking = Prompting::new("y\n".as_bytes(), &mut said);
        let mut destination = Destination::open(&scratch.settings()).expect("a document");
        let offered = offer(
            &mut asking,
            scratch.found(),
            &mut destination,
            &Managed::default(),
            &|_| None,
        );
        assert_eq!(offered.imported, [Source::ClaudeCode]);

        let meanwhile = r#"{"theme": "light"}"#;
        scratch.write(".bravebot/settings.json", meanwhile);
        let problem = write(&destination).expect_err("written over");

        assert!(
            problem.contains("changed while the import was asking"),
            "{problem}"
        );
        assert_eq!(
            std::fs::read_to_string(scratch.settings()).expect("read"),
            meanwhile
        );
    }

    /// IMPORT-3: an opencode model is served by the entry imported beside it. Where that entry is
    /// not written, the file's own entry or the managed list would be asked for a model neither
    /// lists.
    #[test]
    fn an_opencode_model_is_written_only_beside_its_entry() {
        let scratch = Scratch::new("cli-import-model-entry");
        scratch.write(
            ".config/opencode/opencode.json",
            r#"{"model": "amazon-bedrock/openai.gpt-5.6-sol", "provider": {"amazon-bedrock": {"options": {"region": "us-west-2"}}}}"#,
        );
        let own = r#"{"provider": {"amazon-bedrock": {"options": {"region": "eu-west-1"}}}}"#;
        scratch.write(".bravebot/settings.json", own);

        let (said, offered) = asked(&scratch, "y\n", &Managed::default());

        assert!(!offered.asked, "{said}");
        assert!(!said.contains("model:"), "{said}");
        assert_eq!(
            std::fs::read_to_string(scratch.settings()).expect("read"),
            own
        );

        let _ = std::fs::remove_file(scratch.settings());
        let pinning = scratch.write("managed.json", r#"{"provider": {}}"#);
        let (said, offered) = asked(&scratch, "y\n", &Managed::at(&pinning));

        assert!(!offered.asked, "{said}");
        assert!(!said.contains("model:"), "{said}");
        assert!(!scratch.settings().exists(), "{said}");
    }

    /// IMPORT-7: rewriting a file that does not parse would lose what the person wrote in it.
    #[test]
    fn a_settings_file_that_does_not_parse_is_not_rewritten() {
        let scratch = Scratch::new("cli-import-unparsed");
        let broken = r#"{"env": {"AWS_REGION": "eu-central-1",}"#;
        scratch.write(".bravebot/settings.json", broken);

        assert!(Destination::open(&scratch.settings()).is_err());
        assert_eq!(
            std::fs::read_to_string(scratch.settings()).expect("read"),
            broken
        );
    }

    /// IMPORT-7: the file can come to hold a key, so it is never readable by anybody else, not even
    /// for the length of the write.
    #[cfg(unix)]
    #[test]
    fn the_written_file_is_readable_only_by_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = Scratch::new("cli-import-mode");
        scratch.write(".config/opencode/opencode.json", OPENROUTER_HELD);
        let settings = scratch.write(".bravebot/settings.json", "{}");
        std::fs::set_permissions(&settings, std::fs::Permissions::from_mode(0o644))
            .expect("loosen");

        asked(&scratch, "y\ny\n", &Managed::default());

        let mode = std::fs::metadata(&settings)
            .expect("written")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "{mode:o}");
        let leftovers: Vec<_> = std::fs::read_dir(settings.parent().expect("a parent"))
            .expect("list")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .filter(|name| name != "settings.json")
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    /// IMPORT-7: a name the managed layer pins would be overridden by it, so offering it would be
    /// offering a setting that does nothing.
    #[test]
    fn a_name_managed_pins_is_not_offered() {
        let scratch = Scratch::new("cli-import-pinned");
        scratch.write(".claude/settings.json", BEDROCK);
        scratch.write(
            ".config/opencode/opencode.json",
            r#"{"provider": {"gw": {"options": {"baseURL": "https://gw.example.invalid/v1"}}}}"#,
        );
        let pinning = scratch.write(
            "managed.json",
            r#"{"env": {"AWS_REGION": "us-east-1"}, "provider": {}}"#,
        );
        let managed = Managed::at(&pinning);

        let (said, _) = asked(&scratch, "y\ny\n", &managed);

        assert!(!said.contains(r#"env.AWS_REGION: "#), "{said}");
        assert!(!said.contains("Import this from opencode?"), "{said}");
        let pinned = said.find("Not offered").expect("the pins were named");
        assert!(said[pinned..].contains("env.AWS_REGION"), "{said}");
        assert!(said.contains(&pinning.display().to_string()), "{said}");
        let pinned_gateway = said
            .rfind("Not offered")
            .expect("the gateway pin was named");
        assert!(said[pinned_gateway..].contains("provider.gw"), "{said}");
        let file = written(&scratch);
        assert!(!file.contains("AWS_REGION"), "{file}");
        assert!(!file.contains("provider"), "{file}");
    }
}
