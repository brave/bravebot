//! The connector definitions a manifest run may use, and what a plan's connector steps have to
//! satisfy before the plan is put to anybody.
//!
//! Nothing here names a service. A definition's surface says which resources exist, which fields
//! each takes, the form each field's value must have, the values a field may take and the selector
//! terms a fetch accepts, and this compares a plan's text against it. Comparing text a person put
//! in their own directory against text the planner wrote from the task alone decides nothing from
//! anything a service sent.

use bravebot_config::connectors::{Connector, Resource, StepKind, forms};
use bravebot_core::manifest::{
    Arg, CONNECTOR_ACT, CONNECTOR_FETCH, Draft, Manifest, Step, Tier, is_connector,
};
use bravebot_core::selector;
use bravebot_core::slot::SlotId;
use bravebot_core::value::Labelled;
use std::collections::BTreeMap;
use std::path::Path;

/// What a run knows about connectors: the installed definitions, and what it learnt as it ran.
#[derive(Debug, Default)]
pub struct Connectors {
    installed: BTreeMap<String, Result<Connector, String>>,
    run: String,
    receipts: BTreeMap<SlotId, String>,
    /// The detail an authority reported about a call, held for the person's screen.
    heard: Vec<(String, Labelled<String>)>,
    /// The run's stop request, which an authority call waiting on a person or a service honours.
    cancel: bravebot_core::cancel::Cancel,
}

impl Connectors {
    /// Have the authority calls this run makes stop when `cancel` is set.
    pub fn stop_with(&mut self, cancel: bravebot_core::cancel::Cancel) {
        self.cancel = cancel;
    }

    pub fn cancel(&self) -> &bravebot_core::cancel::Cancel {
        &self.cancel
    }

    /// The definitions under `connectors/` in the person's own state directory. With no state
    /// directory there are none, rather than some a guess found.
    pub fn load(home: Option<&Path>) -> Self {
        Self {
            installed: home
                .map(bravebot_config::connectors::installed)
                .unwrap_or_default(),
            run: bravebot_connector::run_id(),
            ..Self::default()
        }
    }

    #[cfg(test)]
    pub fn from(installed: BTreeMap<String, Result<Connector, String>>) -> Self {
        Self {
            installed,
            run: "test-run".to_string(),
            ..Self::default()
        }
    }

    fn usable(&self) -> impl Iterator<Item = &Connector> {
        self.installed.values().filter_map(|c| c.as_ref().ok())
    }

    /// The definitions that are installed but could not be read, each with why, for the planner
    /// and the goal call. Without it a refused definition was simply absent, and the run said no
    /// such connector existed.
    pub fn refused(&self) -> String {
        let mut lines = String::new();
        for (name, why) in self
            .installed
            .iter()
            .filter_map(|(name, c)| c.as_ref().err().map(|why| (name, why)))
        {
            if lines.is_empty() {
                lines.push_str(
                    "\nConnectors installed but refused, so not usable. If the task needs one, say \
                     it is refused and why, not that it is missing:\n",
                );
            }
            let why: String = why.chars().take(300).collect();
            lines.push_str(&format!("  {name}: {why}\n"));
        }
        lines
    }

    /// Keep the detail an authority reported about a call, for the person to read. A fetch keeps
    /// it where the call was not accepted, and an act keeps it whatever the outcome.
    ///
    /// It may quote the service, so it arrives labelled as the connector's output is and goes to
    /// a screen and nowhere else: never into the step's own failure line, which is the driver's
    /// words, and never into a model's context.
    pub fn heard(&mut self, origin: String, detail: Labelled<String>) {
        self.heard.push((origin, detail));
    }

    /// What was heard since the last time this was asked.
    pub fn take_heard(&mut self) -> Vec<(String, Labelled<String>)> {
        std::mem::take(&mut self.heard)
    }

    /// Whether a plan may use any connector at all.
    pub fn any(&self) -> bool {
        self.usable().next().is_some()
    }

    /// The connector a step names, or why it cannot be used.
    pub fn connector(&self, name: &str) -> Result<&Connector, String> {
        match self.installed.get(name) {
            Some(Ok(connector)) => Ok(connector),
            Some(Err(why)) => Err(format!("connector '{name}' could not be read: {why}")),
            None => Err(format!("no connector named '{name}' is installed")),
        }
    }

    pub fn run(&self) -> &str {
        &self.run
    }

    pub fn keep_receipt(&mut self, slot: SlotId, receipt: String) {
        self.receipts.insert(slot, receipt);
    }

    pub fn receipt(&self, slot: &SlotId) -> Option<&str> {
        self.receipts.get(slot).map(String::as_str)
    }

    /// What the goal call is told about the installed connectors: each one's name and what its
    /// resources do, in the definition's own summaries, and no step or field name, since that call
    /// describes the work and names no tools.
    pub fn in_plain_words(&self) -> String {
        let mut lines = String::from("Connectors:\n");
        for connector in self.usable() {
            let summaries: Vec<&str> = connector
                .surface
                .resources
                .values()
                .map(|resource| resource.summary.as_str())
                .collect();
            lines.push_str(&format!("  {}: {}\n", connector.name, summaries.join("; ")));
        }
        lines
    }

    /// What the planner is told about the installed connectors, under the two capabilities.
    pub fn catalogue(&self) -> String {
        let mut lines = String::from(
            "\nConnectors installed. Name them in connector and resource exactly as written. A select \
             is key:value terms separated by spaces; a value with a space in it is double-quoted, \
             as key:\"two words\":\n",
        );
        // A plan fixes dates and times before anything runs, so "tomorrow" has to become a date
        // here: the planner is told today's date and, where this machine names it, its time zone.
        lines.push_str(&format!(
            "  Today's date is {} (UTC)",
            crate::preamble::today()
        ));
        match local_zone() {
            Some(zone) => lines.push_str(&format!("; the person's time zone is {zone}.\n")),
            None => lines.push_str(".\n"),
        }
        for connector in self.usable() {
            lines.push_str(&format!("  {}:\n", connector.name));
            for (name, resource) in &connector.surface.resources {
                lines.push_str(&format!(
                    "    {name} ({}) - {}. uses: {}\n",
                    tool_for(resource.step).to_ascii_uppercase(),
                    resource.summary,
                    resource.uses.join(", ")
                ));
                if !resource.select.is_empty() {
                    lines.push_str(&format!(
                        "      select terms: {}\n",
                        resource.select.join(" ")
                    ));
                }
                if resource.step == StepKind::Fetch && !resource.narrow.is_empty() {
                    lines.push_str(&format!(
                        "      select must use one of: {}\n",
                        resource.narrow.join(" ")
                    ));
                }
                if !resource.bound.is_empty() {
                    lines.push_str(&format!(
                        "      filled in from the fetch named in on, so never give: {}\n",
                        described(&resource.bound, |field, words| format!("{field} ({words})"))
                    ));
                }
                // What an act takes, after the lines every resource has.
                if resource.step == StepKind::Act {
                    let narrows = !resource.narrow.is_empty();
                    let takes_on = resource.uses_field("on");
                    if !resource.text {
                        let no_text = "      takes no text: give neither contents nor from_slot\n";
                        lines.push_str(no_text);
                    }
                    if narrows && resource.uses_field("select") {
                        lines.push_str(&format!(
                            "      acts on every match, so select must use one of: {}\n",
                            resource.narrow.join(" ")
                        ));
                    }
                    if takes_on {
                        lines.push_str(&format!(
                            "      on names the out_slot of one of these fetches: {}; to act on an item you can name, give item instead where uses allows it\n",
                            connector
                                .surface
                                .resources
                                .iter()
                                .filter(|(_, r)| r.step == StepKind::Fetch && r.receipt)
                                .map(|(n, _)| n.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                    if narrows && takes_on {
                        lines.push_str(&format!(
                            "      the fetch named in on either names its item or its select uses one of: {}; a selector with none of them is refused\n",
                            resource.narrow.join(" ")
                        ));
                    }
                }
                for (option, terms) in &resource.narrow_when {
                    lines.push_str(&format!(
                        "      with option {option}, the fetch named in on either names its item or its select uses one of: {}; anything else is refused\n",
                        terms.join(" ")
                    ));
                }
                for (field, values) in &resource.values {
                    lines.push_str(&format!(
                        "      {field} here is one of: {}\n",
                        values.join(", ")
                    ));
                }
                if let Some(example) = connector.surface.examples.get(name) {
                    lines.push_str(&format!("      example: {example}\n"));
                }
            }
            for (field, values) in &connector.surface.values {
                let open = if values.iter().any(|v| v.ends_with('*')) {
                    " (a trailing * stands for any name you give in its place)"
                } else {
                    ""
                };
                lines.push_str(&format!(
                    "    {field} is one of: {}{open}\n",
                    values.join(", ")
                ));
            }
        }
        lines
    }

    /// The draft with each connector step's omitted fields given the definition's defaults, so
    /// the lock and the line a person approves hold the value that will be sent.
    pub fn with_defaults(&self, mut draft: Draft) -> Draft {
        for step in &mut draft.steps {
            if !is_connector(&step.tool) {
                continue;
            }
            let text = |args: &BTreeMap<String, Arg>, field: &str| {
                args.get(field)
                    .and_then(Arg::text)
                    .unwrap_or_default()
                    .to_string()
            };
            let Ok(connector) = self.connector(&text(&step.args, "connector")) else {
                continue;
            };
            let Some(resource) = connector
                .surface
                .resources
                .get(&text(&step.args, "resource"))
            else {
                continue;
            };
            // Given in any shape, a number or a list included: one the plan gave is never replaced,
            // and the lock refuses it if it is not text.
            let given = |args: &BTreeMap<String, Arg>, field: &str| match args.get(field) {
                Some(Arg::Text(value)) => !value.trim().is_empty(),
                Some(Arg::List(values)) => !values.is_empty(),
                Some(Arg::Count(_)) => true,
                None => false,
            };
            for (field, value) in &connector.surface.defaults {
                // A field that is half of an either-or pair is left empty when the other half
                // is given, or the step would hold both.
                let Some(entry) = resource
                    .alternatives()
                    .find(|fields| fields.contains(&field.as_str()))
                else {
                    continue;
                };
                let alternative_given = entry.iter().any(|f| f != field && given(&step.args, f));
                if !alternative_given && !given(&step.args, field) {
                    step.args.insert(field.clone(), Arg::Text(value.clone()));
                }
            }
        }
        draft
    }

    /// For each connector act, the words the plan line adds: the text and title the plan wrote,
    /// where it wrote them, and what the definition fills in from the item the act lands on and
    /// from which fetch. Text a slot carries is not known until the run, and the release on the
    /// line already names where it comes from. The words are the plan's and the definition's;
    /// nothing here names a service.
    pub fn notes(&self, plan: &Manifest) -> BTreeMap<usize, String> {
        let mut notes = BTreeMap::new();
        for (index, step) in plan.steps().iter().enumerate() {
            if step.tool() != CONNECTOR_ACT {
                continue;
            }
            let resource = self
                .connector(step.text("connector"))
                .ok()
                .and_then(|connector| connector.surface.resources.get(step.text("resource")));
            let mut said = Vec::new();
            if resource.is_some_and(|resource| resource.high_impact) {
                said.push("high-impact write".to_string());
            }
            for (field, words) in [("contents", "sends"), ("title", "title")] {
                if !step.text(field).is_empty() {
                    said.push(format!("{words}: “{}”", one_line(step.text(field))));
                }
            }
            let bound = resource
                .map(|resource| &resource.bound)
                .filter(|bound| !bound.is_empty());
            if let Some(bound) = bound {
                said.push(format!(
                    "the authority fills in, from the item '{}' chose: {}",
                    step.text("on"),
                    described(bound, |field, words| format!("{field} = {words}"))
                ));
            }
            if !said.is_empty() {
                notes.insert(index, said.join("; "));
            }
        }
        notes
    }

    /// Hold every connector step in a frozen plan to its definition's surface.
    pub fn check(&self, plan: &Manifest) -> Result<(), String> {
        for (index, step) in plan.steps().iter().enumerate() {
            if !is_connector(step.tool()) {
                continue;
            }
            self.check_step(plan, step)
                .map_err(|why| format!("step {}: {why}", index + 1))?;
        }
        Ok(())
    }

    fn check_step(&self, plan: &Manifest, step: &Step) -> Result<(), String> {
        let connector = self.connector(step.text("connector"))?;
        let surface = &connector.surface;
        let name = step.text("resource");
        let resource = surface
            .resources
            .get(name)
            .ok_or_else(|| format!("'{}' offers no resource '{name}'", connector.name))?;
        if step.tool() != tool_for(resource.step) {
            return Err(format!("'{name}' is not a resource this step can use"));
        }

        let given = |field: &str| !step.text(field).trim().is_empty();
        for field in resource.bound.keys() {
            if given(field) {
                return Err(format!(
                    "'{name}' takes '{field}' from the item it acts on; the plan does not give it"
                ));
            }
        }
        for field in ["place", "item", "select", "path", "option", "on", "title"] {
            if given(field) && !resource.uses_field(field) {
                return Err(format!("'{name}' does not take '{field}'"));
            }
        }
        for alternatives in resource.alternatives() {
            let present = alternatives.iter().filter(|f| given(f)).count();
            let fine = match alternatives.as_slice() {
                // A fetch's selector only narrows what it reads, as the authority holds it too.
                [only] => present == 1 || (resource.step == StepKind::Fetch && *only == "select"),
                _ => present == 1,
            };
            if !fine {
                return Err(format!(
                    "'{name}' needs exactly one of {}",
                    alternatives.join(" or ")
                ));
            }
        }

        // Every field given that the definition names a form for is held to it, as the authority
        // holds every field it uses.
        let formed = resource.forms.keys().chain(surface.forms.keys());
        for field in formed.filter(|field| given(field)) {
            let form = resource
                .forms
                .get(field)
                .or_else(|| surface.forms.get(field));
            if let Some(form) = form {
                forms::check(form, step.text(field)).map_err(|why| format!("'{field}' {why}"))?;
            }
        }
        let limited = surface
            .values
            .iter()
            .map(|(field, values)| (field, resource.values.get(field).unwrap_or(values)));
        let own = resource
            .values
            .iter()
            .filter(|(field, _)| !surface.values.contains_key(*field));
        for (field, values) in limited.chain(own) {
            if given(field) && !values.iter().any(|v| value_matches(v, step.text(field))) {
                return Err(format!("'{field}' must be one of {}", values.join(", ")));
            }
        }
        let terms = selector::terms(step.text("select"))?;
        for (key, value) in &terms {
            if !resource
                .select
                .iter()
                .any(|pattern| selector::matches(pattern, key, value))
            {
                return Err(format!(
                    "'{name}' does not take the selector term '{key}:{value}'"
                ));
            }
        }
        // A fetch the definition holds to narrowing, such as a search across every place, uses one
        // of its terms.
        if resource.step == StepKind::Fetch
            && !resource.narrow.is_empty()
            && !uses_one_of(&terms, &resource.narrow)
        {
            return Err(format!(
                "'{name}' has to be narrowed with one of {}",
                resource.narrow.join(" ")
            ));
        }

        // The step an act lands on, with its resource: the fetch named in `on`, or the act itself.
        let mut target = (step, resource);
        if given("on") {
            let on = SlotId::new(step.text("on"));
            let fetch = plan
                .steps()
                .iter()
                .find(|s| s.out_slot() == Some(&on) && s.tier() == Tier::Fetch)
                .ok_or_else(|| format!("'on' names '{on}', which no fetch filled"))?;
            // The item an act lands on comes from a fetch of its own connector and place, so it
            // cannot reach outside the place the plan locked for the act.
            if fetch.tool() != CONNECTOR_FETCH
                || fetch.text("connector") != step.text("connector")
                || fetch.text("place") != step.text("place")
            {
                return Err(format!(
                    "'on' names '{on}', which no fetch from this act's connector and place filled"
                ));
            }
            let fetched = surface
                .resources
                .get(fetch.text("resource"))
                .ok_or_else(|| {
                    format!("'on' names '{on}', which no resource of this connector filled")
                })?;
            if !fetched.receipt {
                return Err(format!(
                    "'on' names '{on}', filled by '{}', which chooses no single item",
                    fetch.text("resource")
                ));
            }
            if !resource.narrow.is_empty() && !names_or_narrows(fetched, fetch, &resource.narrow)? {
                return Err(format!(
                    "the fetch it acts on has to name its item or narrow its choice with one of {}",
                    resource.narrow.join(" ")
                ));
            }
            target = (fetch, fetched);
        }
        // An option the definition holds to stricter narrowing, such as an approval, lands only
        // on an item a fetch named or chose by one of its terms.
        // A key may be one of the definition's option patterns, as `add_label:*` is, matched as the
        // authority matches it.
        let option = step.text("option");
        for (_, wanted) in resource
            .narrow_when
            .iter()
            .filter(|(key, _)| !option.is_empty() && value_matches(key, option))
        {
            if !names_or_narrows(target.1, target.0, wanted)? {
                return Err(format!(
                    "'{name}' with {option} has to land on an item a fetch chose by one of {}, or one it named",
                    wanted.join(" ")
                ));
            }
        }
        if resource.step == StepKind::Act {
            let carried = ["contents", "from_slot"]
                .iter()
                .filter(|f| given(f))
                .count();
            match (resource.text, carried) {
                (true, 1) | (false, 0) => {}
                (true, _) => {
                    return Err(format!(
                        "'{name}' needs exactly one of contents or from_slot"
                    ));
                }
                (false, _) => {
                    return Err(format!(
                        "'{name}' takes no text; its option says what it does"
                    ));
                }
            }
            // An act on every match of its own selector lands on all of them, so the selector
            // has to narrow as a resolve's must.
            if given("select")
                && !resource.narrow.is_empty()
                && !uses_one_of(&terms, &resource.narrow)
            {
                return Err(format!(
                    "'{name}' acts on every match, so its selector has to narrow with one of {}",
                    resource.narrow.join(" ")
                ));
            }
        }
        Ok(())
    }
}

/// The time zone this machine says it is in, as an IANA name such as `Europe/London`: where
/// `/etc/localtime` links into a zoneinfo directory, else `TZ`. Only a name with the `time-zone`
/// form is told, so whatever the link holds cannot reach the planner as anything else.
fn local_zone() -> Option<String> {
    let from_link = std::fs::read_link("/etc/localtime")
        .ok()
        .and_then(|target| {
            let target = target.to_string_lossy().into_owned();
            target
                .split_once("zoneinfo/")
                .map(|(_, zone)| zone.to_string())
        });
    from_link
        .or_else(|| std::env::var("TZ").ok())
        .map(|zone| zone.trim_start_matches(':').to_string())
        .filter(|zone| forms::check("time-zone", zone).is_ok())
}

/// Whether a value is one a definition's list offers, where a trailing `*` stands for any
/// non-empty rest.
fn value_matches(pattern: &str, value: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => value.len() > prefix.len() && value.starts_with(prefix),
        None => pattern == value,
    }
}

/// The manifest tool a resource's step is planned as.
fn tool_for(step: StepKind) -> &'static str {
    match step {
        StepKind::Fetch => CONNECTOR_FETCH,
        StepKind::Act => CONNECTOR_ACT,
    }
}

/// The first 200 characters of text the plan wrote, on one line: a control character or a line or
/// paragraph separator is shown as its escape, so the text cannot draw a line of its own.
fn one_line(text: &str) -> String {
    let mut shown = String::new();
    for c in text.chars().take(200) {
        if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') {
            shown.extend(c.escape_default());
        } else {
            shown.push(c);
        }
    }
    if text.chars().count() > 200 {
        shown.push('…');
    }
    shown
}

/// Whether a selector's terms use one of a definition's narrowing terms.
fn uses_one_of(terms: &[(String, String)], narrow: &[String]) -> bool {
    terms
        .iter()
        .any(|(key, value)| narrow.iter().any(|p| selector::matches(p, key, value)))
}

/// Whether a step names its item, where its resource takes one, or its select uses one of
/// `narrow`, so an act landing on what it chose does not land on whichever item sorted first. A
/// step with neither, an empty select included, does not narrow.
fn names_or_narrows(resource: &Resource, step: &Step, narrow: &[String]) -> Result<bool, String> {
    if resource.uses_field("item") && !step.text("item").trim().is_empty() {
        return Ok(true);
    }
    Ok(uses_one_of(&selector::terms(step.text("select"))?, narrow))
}

/// Each field with its words, shaped by `each`, joined as one line.
fn described(fields: &BTreeMap<String, String>, each: fn(&str, &str) -> String) -> String {
    fields
        .iter()
        .map(|(field, words)| each(field, words))
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_config::connectors::Surface;
    use bravebot_core::capability::CapabilitySet;
    use bravebot_core::event::RecordingSink;
    use bravebot_core::manifest::{CONNECTOR_FETCH, DraftStep};
    use bravebot_core::policy::{Policy, ReleasePlan, Routing};
    use bravebot_core::value::Labelled;

    /// A draft validated the way a run validates one: adopted from a trusted proposal.
    fn validate_for_tests(draft: &Draft) -> Result<Manifest, String> {
        let mut sink = RecordingSink::new();
        let mut routing = Routing::new();
        routing.insert_trusted("task", "test");
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::none(),
            &mut sink,
        )
        .map_err(|d| d.to_string())?;
        policy
            .adopt_manifest(&Labelled::trusted(draft.clone()))
            .map_err(|d| d.to_string())
    }

    fn connector(name: &str, surface: serde_json::Value) -> Connector {
        Connector {
            name: name.into(),
            definition_id: "0".repeat(64),
            authority: "/opt/authority/agent".into(),
            item: format!("svc:{name}:me"),
            surface: serde_json::from_value::<Surface>(surface).unwrap(),
        }
    }

    /// A definition with fetches and acts of every shape the checks tell apart.
    fn github() -> Connector {
        connector(
            "github",
            serde_json::json!({
                "resources": {
                    "issue": { "step": "fetch", "uses": ["place", "item"], "summary": "one issue" },
                    "pull": { "step": "fetch", "uses": ["place", "item"], "summary": "one pull", "receipt": true },
                    "find_issue": { "step": "fetch", "uses": ["place", "select"], "summary": "first issue",
                        "select": ["state:*", "label:*", "no:label", "sort:created-*"], "receipt": true },
                    "find_pull": { "step": "fetch", "uses": ["place", "select"], "summary": "first pull",
                        "select": ["state:*", "head:*", "label:*", "sort:created-*", "sort:updated-*"],
                        "receipt": true },
                    "list": { "step": "fetch", "uses": ["place", "select"], "summary": "many",
                        "select": ["state:*", "sort:comments-*"] },
                    "search": { "step": "fetch", "uses": ["select"], "summary": "across places",
                        "select": ["text:*", "user:*"] },
                    "comment": { "step": "act", "uses": ["place", "item|on"], "summary": "comment" },
                    "review": { "step": "act", "uses": ["place", "on", "option"], "summary": "review",
                        "high_impact": true,
                        "narrow": ["head:*", "sort:created-*", "sort:updated-*"],
                        "narrow_when": { "APPROVE": ["head:*"] } },
                    "modify": { "step": "act", "uses": ["place", "item|on", "option"], "summary": "close",
                        "text": false, "values": { "option": ["close", "reopen"] } },
                    "put_file": { "step": "act", "uses": ["place", "item", "path", "title"], "summary": "commit",
                        "forms": { "item": "branch" } },
                    "reply": { "step": "act", "uses": ["place", "on"], "summary": "answer the item it acts on",
                        "bound": { "recipient": "the author of that item", "title": "Re: and that item's title" } }
                },
                "values": { "option": ["COMMENT", "REQUEST_CHANGES", "APPROVE"] },
                "defaults": { "option": "COMMENT" },
                "forms": { "place": "owner/name", "item": "positive-integer", "path": "relative-path" }
            }),
        )
    }

    fn connectors() -> Connectors {
        Connectors::from(BTreeMap::from([("github".to_string(), Ok(github()))]))
    }

    fn fetch(resource: &str, out: &str) -> DraftStep {
        DraftStep::new(CONNECTOR_FETCH)
            .with_text("connector", "github")
            .with_text("resource", resource)
            .with_text("place", "brave/bravebot")
            .with_text("out_slot", out)
    }

    fn act(resource: &str) -> DraftStep {
        DraftStep::new(CONNECTOR_ACT)
            .with_text("connector", "github")
            .with_text("resource", resource)
            .with_text("place", "brave/bravebot")
            .with_text("contents", "text")
    }

    fn checked(steps: Vec<DraftStep>) -> Result<(), String> {
        let connectors = connectors();
        let draft = connectors.with_defaults(Draft::new(steps));
        let plan = validate_for_tests(&draft).map_err(|e| e.to_string())?;
        connectors.check(&plan)
    }

    /// A search across places names none, and a resource that reads from one place needs it, as the
    /// authority holds them, so a plan never sends a place a resource ignores or omits one it needs.
    #[test]
    fn a_fetch_names_a_place_only_when_its_resource_uses_one() {
        let anywhere = |resource: &str| {
            DraftStep::new(CONNECTOR_FETCH)
                .with_text("connector", "github")
                .with_text("resource", resource)
                .with_text("select", "text:timeout user:me")
                .with_text("out_slot", "found")
        };
        assert!(checked(vec![anywhere("search")]).is_ok());
        assert!(
            checked(vec![
                fetch("search", "found").with_text("select", "text:timeout")
            ])
            .unwrap_err()
            .contains("does not take 'place'")
        );
        assert!(
            checked(vec![anywhere("list").with_text("select", "state:open")])
                .unwrap_err()
                .contains("needs exactly one of place")
        );
    }

    /// The goal call describes the work and names no tools, so it is told what each connector does in
    /// the definition's words.
    #[test]
    fn the_goal_call_is_told_each_connector_and_what_it_does_in_plain_words() {
        let told = connectors().in_plain_words();
        assert!(told.starts_with("Connectors:\n  github: "), "{told}");
        assert!(told.contains("one issue"), "{told}");
        assert!(told.contains("comment"), "{told}");
        assert!(
            !told.contains("CONNECTOR_"),
            "the goal call names no tools: {told}"
        );
        assert!(!told.contains("uses:"), "{told}");
    }

    /// Every field and selector term a step gives is one its resource takes, so nothing reaches the
    /// authority that the definition did not offer.
    #[test]
    fn a_step_is_held_to_its_resource_fields_values_and_selector_terms() {
        assert!(checked(vec![fetch("issue", "i").with_text("item", "914")]).is_ok());
        assert!(
            checked(vec![
                fetch("issue", "i")
                    .with_text("item", "914")
                    .with_text("select", "state:open")
            ])
            .unwrap_err()
            .contains("does not take 'select'")
        );
        assert!(
            checked(vec![fetch("issue", "i")])
                .unwrap_err()
                .contains("exactly one of item")
        );
        assert!(
            checked(vec![
                fetch("find_pull", "p").with_text("select", "sort:comments-desc")
            ])
            .unwrap_err()
            .contains("sort:comments-desc")
        );
        assert!(
            checked(vec![
                fetch("find_issue", "i")
                    .with_text("select", "state:open label:\"good first issue\"")
            ])
            .is_ok()
        );
        assert!(
            checked(vec![fetch("nothing", "i")])
                .unwrap_err()
                .contains("no resource")
        );
        assert!(
            checked(vec![act("issue").with_text("item", "1")])
                .unwrap_err()
                .contains("not a resource this step can use")
        );
    }

    /// A mail definition goes through the same check with no item form of its own, so a path or a query
    /// cannot pass as a message id.
    #[test]
    fn mail_reads_take_a_selector_or_an_opaque_id_and_nothing_else() {
        let surface: Surface = serde_json::from_value(serde_json::json!({
            "resources": {
                "inbox": { "step": "fetch", "uses": ["place", "select"], "summary": "recent mail",
                           "forms": { "place": "opaque-id" },
                           "select": ["in:inbox", "is:unread", "from:*", "limit:*"] },
                "message": { "step": "fetch", "uses": ["place", "item"], "summary": "one email",
                             "forms": { "place": "opaque-id" } },
                "send": { "step": "act", "uses": ["place", "title"], "summary": "send one email" }
            },
            "forms": { "place": "address", "item": "opaque-id" },
            "defaults": { "place": "me" }
        }))
        .unwrap();
        let gmail = Connector {
            name: "gmail".into(),
            definition_id: "0".repeat(64),
            authority: "/opt/authority/agent".into(),
            item: "svc:gmail:me@example.com".into(),
            surface,
        };
        let connectors = Connectors::from(BTreeMap::from([("gmail".to_string(), Ok(gmail))]));
        let check = |resource: &str, field: &str, value: &str| {
            let step = DraftStep::new(CONNECTOR_FETCH)
                .with_text("connector", "gmail")
                .with_text("resource", resource)
                .with_text(field, value)
                .with_text("out_slot", "m");
            let draft = connectors.with_defaults(Draft::new(vec![step]));
            let plan = validate_for_tests(&draft)?;
            connectors.check(&plan)
        };
        let r = check("inbox", "select", r#"in:inbox from:"a@b.c" limit:5"#);
        assert!(r.is_ok(), "{r:?}");
        assert!(check("inbox", "select", "in:spam").is_err());
        assert!(check("message", "item", "18c1f2a3b4d5e6f7").is_ok());
        for bad in ["../settings", "a.b", "a?b"] {
            assert!(check("message", "item", bad).is_err(), "{bad} passed");
        }
        // The default mailbox is no recipient: a send that names none is refused, not sent to it.
        let send = DraftStep::new(CONNECTOR_ACT)
            .with_text("connector", "gmail")
            .with_text("resource", "send")
            .with_text("title", "Hi")
            .with_text("contents", "text");
        let plan = validate_for_tests(&connectors.with_defaults(Draft::new(vec![send]))).unwrap();
        assert!(connectors.check(&plan).unwrap_err().contains("address"));
    }

    /// A default fills a field the plan left out, and never the second half of an either-or the plan
    /// already gave, which would make the step ask for both.
    #[test]
    fn a_default_is_not_filled_beside_the_other_half_of_an_either_or() {
        let mail = connector(
            "mail",
            serde_json::json!({
                "resources": {
                    "message": { "step": "fetch", "uses": ["item|select"], "summary": "one email",
                                 "select": ["from:*"] }
                },
                "forms": { "item": "opaque-id" },
                "defaults": { "item": "latest" }
            }),
        );
        let connectors = Connectors::from(BTreeMap::from([("mail".to_string(), Ok(mail))]));
        let step = |select: Option<&str>| {
            let mut s = DraftStep::new(CONNECTOR_FETCH)
                .with_text("connector", "mail")
                .with_text("resource", "message")
                .with_text("out_slot", "m");
            if let Some(select) = select {
                s = s.with_text("select", select);
            }
            connectors.with_defaults(Draft::new(vec![s]))
        };
        assert!(!step(Some("from:a")).steps[0].args.contains_key("item"));
        assert!(step(None).steps[0].args.contains_key("item"));
    }

    /// A value the plan gave is never replaced by a default, whatever its shape: a number stays
    /// the number, for the lock to refuse as not text, rather than the default standing in for it.
    #[test]
    fn a_default_never_replaces_a_value_the_plan_gave_in_another_shape() {
        let mail = connector(
            "mail",
            serde_json::json!({
                "resources": {
                    "message": { "step": "fetch", "uses": ["item"], "summary": "one email" }
                },
                "defaults": { "item": "latest" }
            }),
        );
        let connectors = Connectors::from(BTreeMap::from([("mail".to_string(), Ok(mail))]));
        let mut step = DraftStep::new(CONNECTOR_FETCH)
            .with_text("connector", "mail")
            .with_text("resource", "message")
            .with_text("out_slot", "m");
        step.args.insert("item".into(), Arg::Count(2));
        let filled = connectors.with_defaults(Draft::new(vec![step]));
        assert_eq!(filled.steps[0].args.get("item"), Some(&Arg::Count(2)));
    }

    /// A value is held to its form before the plan is put to anybody, so a place that climbs out of its
    /// owner/name, or an id with a leading zero, never reaches the line a person approves.
    #[test]
    fn a_value_outside_its_named_form_is_refused_before_anything_is_asked() {
        for place in ["x/../../user", "a/..", "a/b/c", "a b/c", "x%2e%2e"] {
            let refused = checked(vec![
                DraftStep::new(CONNECTOR_FETCH)
                    .with_text("connector", "github")
                    .with_text("resource", "issue")
                    .with_text("place", place)
                    .with_text("item", "1")
                    .with_text("out_slot", "i"),
            ]);
            assert!(refused.is_err(), "{place} passed");
        }
        assert!(checked(vec![fetch("issue", "i").with_text("item", "01")]).is_err());
        assert!(
            checked(vec![
                act("put_file")
                    .with_text("item", "bravebot/fix")
                    .with_text("path", "docs/../secret")
                    .with_text("title", "t")
            ])
            .is_err()
        );
        assert!(
            checked(vec![
                act("put_file")
                    .with_text("item", "bravebot/fix")
                    .with_text("path", "docs/x.md")
                    .with_text("title", "t")
            ])
            .is_ok()
        );
    }

    /// The planner is told how to quote a value with a space, and the grammar reads it back as one term.
    #[test]
    fn the_planner_is_told_a_value_with_a_space_is_quoted_and_such_a_term_parses() {
        assert!(
            connectors().catalogue().contains(&format!(
                "Today's date is {} (UTC)",
                crate::preamble::today()
            )),
            "a plan fixes dates before anything runs, so the planner is told today's"
        );
        assert!(
            connectors()
                .catalogue()
                .contains("a value with a space in it is double-quoted, as key:\"two words\""),
        );
        assert_eq!(
            selector::terms("subject:\"connector test\" is:unread").unwrap(),
            vec![
                ("subject".to_string(), "connector test".to_string()),
                ("is".to_string(), "unread".to_string())
            ]
        );
        assert!(selector::terms("subject:connector test").is_err());
    }

    /// A fetch the definition holds to narrowing, such as a search across every place, uses one of its
    /// terms, and the planner is told which.
    #[test]
    fn a_fetch_whose_surface_lists_narrow_terms_uses_one_in_its_select() {
        let mut scoped = connectors();
        if let Some(Ok(github)) = scoped.installed.get_mut("github") {
            let search = github.surface.resources.get_mut("search").unwrap();
            search.select.push("org:*".into());
            search.narrow = vec!["user:*".into(), "org:*".into()];
        }
        let search = |select: &str| {
            let draft = Draft::new(vec![
                DraftStep::new(CONNECTOR_FETCH)
                    .with_text("connector", "github")
                    .with_text("resource", "search")
                    .with_text("select", select)
                    .with_text("out_slot", "found"),
            ]);
            scoped.check(&validate_for_tests(&draft).unwrap())
        };
        assert_eq!(
            search("text:timeout").unwrap_err(),
            "step 1: 'search' has to be narrowed with one of user:* org:*"
        );
        assert!(search("text:timeout org:brave").is_ok());
        assert!(
            scoped
                .catalogue()
                .contains("    search (CONNECTOR_FETCH) - across places. uses: select\n      select terms: text:* user:* org:*\n      select must use one of: user:* org:*\n"),
            "{}",
            scoped.catalogue()
        );
        assert!(
            !connectors()
                .catalogue()
                .contains("      select must use one of"),
            "a fetch with no narrow terms is told nothing about them"
        );
    }

    /// A resource's own list of values replaces the definition's for that resource, and a trailing `*`
    /// stands for a non-empty rest, as the authority reads it.
    #[test]
    fn a_resource_that_lists_its_own_values_takes_only_those() {
        let calendar = connector(
            "calendar",
            serde_json::json!({
                "resources": {
                    "events": { "step": "fetch", "uses": ["place"], "summary": "events" },
                    "primary": { "step": "fetch", "uses": ["place"], "summary": "own events",
                                 "values": { "place": ["primary"] } }
                },
                "values": { "place": ["primary", "team-*"] }
            }),
        );
        let connectors = Connectors::from(BTreeMap::from([("calendar".to_string(), Ok(calendar))]));
        let check = |resource: &str, place: &str| {
            let step = DraftStep::new(CONNECTOR_FETCH)
                .with_text("connector", "calendar")
                .with_text("resource", resource)
                .with_text("place", place)
                .with_text("out_slot", "e");
            connectors.check(&validate_for_tests(&Draft::new(vec![step]))?)
        };
        assert!(check("events", "team-web").is_ok());
        assert!(
            check("events", "team-")
                .unwrap_err()
                .contains("must be one of")
        );
        assert!(check("primary", "primary").is_ok());
        assert!(
            check("primary", "team-web")
                .unwrap_err()
                .contains("must be one of primary")
        );
        let shown = connectors.catalogue();
        assert!(shown.contains("place here is one of: primary"), "{shown}");
        assert!(
            shown.contains("place is one of: primary, team-* (a trailing * stands for"),
            "{shown}"
        );
    }

    /// A definition that could not be read refuses the plan that names it with its reason, and the
    /// planner is told it is refused rather than that it is missing.
    #[test]
    fn a_connector_that_is_not_installed_or_could_not_be_read_refuses_the_plan() {
        let connectors = Connectors::from(BTreeMap::from([(
            "broken".to_string(),
            Err("bad json".to_string()),
        )]));
        let plan = validate_for_tests(&Draft::new(vec![
            DraftStep::new(CONNECTOR_FETCH)
                .with_text("connector", "broken")
                .with_text("resource", "issue")
                .with_text("place", "a/b")
                .with_text("out_slot", "i"),
        ]))
        .unwrap();
        assert!(connectors.check(&plan).unwrap_err().contains("bad json"));
        let elsewhere = validate_for_tests(&Draft::new(vec![
            DraftStep::new(CONNECTOR_FETCH)
                .with_text("connector", "absent")
                .with_text("resource", "issue")
                .with_text("out_slot", "i"),
        ]))
        .unwrap();
        assert!(
            connectors
                .check(&elsewhere)
                .unwrap_err()
                .contains("no connector named 'absent' is installed")
        );
        assert!(!connectors.any());
        let told = connectors.refused();
        assert!(
            told.contains("broken: bad json") && told.contains("refused"),
            "{told}"
        );
        assert!(Connectors::default().refused().is_empty());
    }

    /// The planner is told each resource under the capability that plans it, and the values a field
    /// takes, and no tool name.
    #[test]
    fn the_catalogue_lists_each_resource_under_the_capability_it_uses() {
        let shown = connectors().catalogue();
        assert!(shown.contains("find_issue (CONNECTOR_FETCH) - first issue. uses: place, select"));
        assert!(shown.contains("comment (CONNECTOR_ACT)"));
        assert!(shown.contains("option is one of: COMMENT, REQUEST_CHANGES"));
        assert!(
            !shown.contains(CONNECTOR_FETCH),
            "a tool name leaked into the catalogue"
        );
    }

    /// An act's own list of values replaces the definition's for its option, so an option another
    /// resource offers is refused here.
    #[test]
    fn an_acts_own_values_replace_the_definitions_for_its_option() {
        let state = |option: &str| {
            vec![
                DraftStep::new(CONNECTOR_ACT)
                    .with_text("connector", "github")
                    .with_text("resource", "modify")
                    .with_text("place", "brave/bravebot")
                    .with_text("item", "4")
                    .with_text("option", option),
            ]
        };
        assert!(checked(state("close")).is_ok());
        assert!(
            checked(state("COMMENT"))
                .unwrap_err()
                .contains("must be one of close, reopen")
        );
        let review = vec![
            fetch("pull", "p").with_text("item", "7"),
            act("review")
                .with_text("on", "p")
                .with_text("option", "close"),
        ];
        assert!(checked(review).unwrap_err().contains("COMMENT"));
        assert!(
            connectors()
                .catalogue()
                .contains("option here is one of: close, reopen")
        );
    }

    /// A definition with no item form is held to the same rules, so a send names its recipients in their
    /// form and nothing slips through for want of one.
    #[test]
    fn a_mail_definition_is_checked_by_the_same_rules_with_no_item() {
        let surface: Surface = serde_json::from_value(serde_json::json!({
            "resources": {
                "send": { "step": "act", "uses": ["place", "title"], "summary": "send one email" }
            },
            "forms": { "place": "address" }
        }))
        .unwrap();
        let gmail = Connector {
            name: "gmail".into(),
            definition_id: "0".repeat(64),
            authority: "/opt/authority/agent".into(),
            item: "svc:gmail:me@example.com".into(),
            surface,
        };
        let connectors = Connectors::from(BTreeMap::from([("gmail".to_string(), Ok(gmail))]));
        let check = |step: DraftStep| {
            let draft = connectors.with_defaults(Draft::new(vec![step]));
            let plan = validate_for_tests(&draft)?;
            connectors.check(&plan)
        };
        let send = |to: &str| {
            DraftStep::new(CONNECTOR_ACT)
                .with_text("connector", "gmail")
                .with_text("resource", "send")
                .with_text("place", to)
                .with_text("contents", "text")
        };
        assert!(check(send("someone@example.com").with_text("title", "Hi")).is_ok());
        assert!(check(send("someone@example.com")).is_err());
        assert!(check(send("someone").with_text("title", "Hi")).is_err());
        assert!(
            check(
                send("someone@example.com")
                    .with_text("title", "Hi")
                    .with_text("item", "1")
            )
            .unwrap_err()
            .contains("does not take 'item'")
        );
    }

    /// A send may name several recipients, and each is one mailbox in its form, so one approved address
    /// cannot carry another.
    #[test]
    fn a_plan_may_send_to_several_but_each_is_one_mailbox() {
        let surface: Surface = serde_json::from_value(serde_json::json!({
            "resources": {
                "send": { "step": "act", "uses": ["place", "title"], "summary": "send one email",
                          "forms": { "place": "addresses" } }
            },
            "forms": { "place": "address" }
        }))
        .unwrap();
        let gmail = Connector {
            name: "gmail".into(),
            definition_id: "0".repeat(64),
            authority: "/opt/authority/agent".into(),
            item: "svc:gmail:me@example.com".into(),
            surface,
        };
        let connectors = Connectors::from(BTreeMap::from([("gmail".to_string(), Ok(gmail))]));
        let check = |to: &str| {
            let send = DraftStep::new(CONNECTOR_ACT)
                .with_text("connector", "gmail")
                .with_text("resource", "send")
                .with_text("place", to)
                .with_text("title", "Notes")
                .with_text("contents", "text");
            let draft = connectors.with_defaults(Draft::new(vec![send]));
            let plan = validate_for_tests(&draft)?;
            connectors.check(&plan)
        };
        assert!(check("bob@example.org").is_ok());
        assert!(check("bob@example.org,carol@example.org").is_ok());
        assert!(
            check("bob@example.org, carol@example.org")
                .unwrap_err()
                .contains("space")
        );
        assert!(check("bob@example.org,bob@example.org").is_err());
        assert!(check("not an address").is_err());
    }

    /// An act whose option says what it does carries no text, and one that lands on every match of its
    /// own selector narrows it, so it cannot land on everything.
    #[test]
    fn an_act_with_no_text_takes_an_option_and_one_on_every_match_narrows_its_selector() {
        let surface: Surface = serde_json::from_value(serde_json::json!({
            "resources": {
                "find_message": { "step": "fetch", "uses": ["place", "select"], "summary": "newest match",
                                  "select": ["from:*", "is:unread"], "receipt": true },
                "reply": { "step": "act", "uses": ["place", "on"], "summary": "reply" },
                "modify": { "step": "act", "uses": ["place", "on|select", "option"], "summary": "label it or all",
                            "text": false, "narrow": ["from:*"], "select": ["from:*", "is:unread"] }
            },
            "values": { "option": ["archive", "add_label:*"] },
            "forms": { "place": "opaque-id" },
            "defaults": { "place": "me" }
        }))
        .unwrap();
        let gmail = Connector {
            name: "gmail".into(),
            definition_id: "0".repeat(64),
            authority: "/opt/authority/agent".into(),
            item: "svc:gmail:me@example.com".into(),
            surface,
        };
        let connectors = Connectors::from(BTreeMap::from([("gmail".to_string(), Ok(gmail))]));
        let check = |steps: Vec<DraftStep>| {
            let draft = connectors.with_defaults(Draft::new(steps));
            let plan = validate_for_tests(&draft)?;
            connectors.check(&plan)
        };
        let found = DraftStep::new(CONNECTOR_FETCH)
            .with_text("connector", "gmail")
            .with_text("resource", "find_message")
            .with_text("select", "from:a@example.com")
            .with_text("out_slot", "mail");
        let on_mail = |resource: &str| {
            DraftStep::new(CONNECTOR_ACT)
                .with_text("connector", "gmail")
                .with_text("resource", resource)
                .with_text("on", "mail")
        };
        let modify = |option: &str| on_mail("modify").with_text("option", option);
        assert!(check(vec![found.clone(), modify("archive")]).is_ok());
        assert!(check(vec![found.clone(), modify("add_label:Receipts")]).is_ok());
        for bad in ["add_label:", "delete"] {
            assert!(
                check(vec![found.clone(), modify(bad)])
                    .unwrap_err()
                    .contains("must be one of"),
                "{bad}"
            );
        }
        assert!(
            check(vec![
                found.clone(),
                modify("archive").with_text("contents", "x")
            ])
            .unwrap_err()
            .contains("takes no text")
        );
        assert!(
            check(vec![found.clone(), on_mail("reply")])
                .unwrap_err()
                .contains("exactly one of contents or from_slot")
        );

        let unread = found.clone().with_text("select", "is:unread");
        assert!(
            check(vec![unread, modify("archive")])
                .unwrap_err()
                .contains("narrow"),
            "a fetch in on narrows as a selector does"
        );

        let every = |select: &str| {
            DraftStep::new(CONNECTOR_ACT)
                .with_text("connector", "gmail")
                .with_text("resource", "modify")
                .with_text("select", select)
                .with_text("option", "archive")
        };
        assert!(check(vec![every("from:news@example.com is:unread")]).is_ok());
        assert!(
            check(vec![every("is:unread")])
                .unwrap_err()
                .contains("acts on every match")
        );
        assert!(
            check(vec![
                found.clone(),
                modify("archive").with_text("select", "from:a@example.com")
            ])
            .unwrap_err()
            .contains("exactly one of on")
        );

        let told = connectors.catalogue();
        assert!(told.contains("takes no text"), "{told}");
        assert!(told.contains("select must use one of: from:*"), "{told}");
        assert!(
            told.contains("the fetch named in on either names its item"),
            "{told}"
        );
        assert!(told.contains("a trailing * stands for"), "{told}");
    }

    /// The planner is told which fetches choose a single item, so `on` names one an act can land on.
    #[test]
    fn the_planner_is_told_which_fetches_an_act_may_name_in_on() {
        let shown = connectors().catalogue();
        assert!(
            shown.contains("on names the out_slot of one of these fetches: "),
            "{shown}"
        );
        assert!(
            shown.contains("to act on an item you can name, give item instead"),
            "{shown}"
        );
    }

    /// The planner is told how the fetch an act lands on has to narrow, so it plans one the check passes.
    #[test]
    fn the_planner_is_told_how_the_fetch_an_act_lands_on_must_narrow() {
        let shown = connectors().catalogue();
        assert!(
            shown.contains("names its item or its select uses one of: head:*"),
            "{shown}"
        );
        let review = |select: &str| {
            vec![
                fetch("find_pull", "p").with_text("select", select),
                act("review")
                    .with_text("on", "p")
                    .with_text("option", "COMMENT"),
            ]
        };
        assert!(checked(review("state:open sort:created-desc")).is_ok());
        assert!(checked(review("head:fix")).is_ok());
        let unnarrowed = checked(review("state:open")).unwrap_err();
        assert!(unnarrowed.contains("narrow"), "{unnarrowed}");
        assert!(
            shown.contains("a selector with none of them is refused"),
            "{shown}"
        );
    }

    /// A fetch an act lands on names its item or narrows by one of the act's terms, so the act does not
    /// land on whichever item sorted first.
    #[test]
    fn a_fetch_an_act_lands_on_narrows_only_by_naming_its_item_or_by_a_narrowing_term() {
        let connectors = connectors();
        let github = connectors.connector("github").unwrap();
        let resources = &github.surface.resources;
        let narrow = &resources["review"].narrow;
        let plan = validate_for_tests(&Draft::new(vec![
            fetch("find_pull", "a"),
            fetch("find_pull", "b").with_text("select", "head:fix"),
            fetch("pull", "c").with_text("item", "7"),
            // An item given to a fetch whose resource takes none does not name what it chose.
            fetch("find_pull", "d").with_text("item", "7"),
        ]))
        .unwrap();
        let narrows = |index: usize, resource: &str| {
            names_or_narrows(&resources[resource], &plan.steps()[index], narrow).unwrap()
        };
        assert!(!narrows(0, "find_pull"), "an empty select narrows nothing");
        assert!(narrows(1, "find_pull"));
        assert!(narrows(2, "pull"));
        assert!(!narrows(3, "find_pull"));

        // A resolve that takes no selector at all chooses whichever item comes first, so an act
        // that has to narrow is refused on it before approval, and lands on one named by number.
        let mut newest =
            Connectors::from(BTreeMap::from([("github".to_string(), Ok(github.clone()))]));
        if let Some(Ok(github)) = newest.installed.get_mut("github") {
            let mut resolve = github.surface.resources["find_pull"].clone();
            resolve.uses = vec!["place".into()];
            resolve.select.clear();
            github
                .surface
                .resources
                .insert("newest_pull".into(), resolve);
        }
        let review = |fetched: DraftStep| {
            let draft = newest.with_defaults(Draft::new(vec![
                fetched,
                act("review").with_text("on", "p"),
            ]));
            newest.check(&validate_for_tests(&draft).unwrap())
        };
        let refused = review(fetch("newest_pull", "p")).unwrap_err();
        assert!(refused.contains("name its item or narrow"), "{refused}");
        assert!(review(fetch("pull", "p").with_text("item", "7")).is_ok());
    }

    /// An act whose effect reaches further than its text says so on the line a person approves.
    #[test]
    fn a_high_impact_act_says_so_on_its_plan_line() {
        let connectors = connectors();
        let draft = connectors.with_defaults(Draft::new(vec![
            fetch("pull", "p").with_text("item", "7"),
            act("review")
                .with_text("on", "p")
                .with_text("contents", "Looks right."),
            act("comment")
                .with_text("item", "7")
                .with_text("contents", "Thanks."),
        ]));
        let plan = validate_for_tests(&draft).unwrap();
        let notes = connectors.notes(&plan);
        assert!(
            notes[&1].starts_with("high-impact write; "),
            "{}",
            notes[&1]
        );
        assert!(!notes[&2].contains("high-impact"), "{}", notes[&2]);
    }

    /// An approval lands only on a pull request chosen by its branch or named, so a label cannot pick
    /// what gets approved.
    #[test]
    fn an_approval_lands_only_on_a_pull_chosen_by_its_branch_or_named() {
        let review = |fetched: DraftStep, option: &str| {
            vec![
                fetched,
                act("review")
                    .with_text("on", "p")
                    .with_text("option", option),
            ]
        };
        let found = |select: &str| fetch("find_pull", "p").with_text("select", select);
        let refused = checked(review(found("label:x sort:updated-desc"), "APPROVE")).unwrap_err();
        assert!(
            refused.contains(
                "'review' with APPROVE has to land on an item a fetch chose by one of head:*, or one it named"
            ),
            "{refused}"
        );
        assert!(checked(review(found("head:fix"), "APPROVE")).is_ok());
        assert!(checked(review(fetch("pull", "p").with_text("item", "7"), "APPROVE")).is_ok());
        assert!(checked(review(found("label:x sort:updated-desc"), "COMMENT")).is_ok());
        assert!(
            connectors().catalogue().contains(
                "with option APPROVE, the fetch named in on either names its item or its select uses one of: head:*"
            )
        );
        let mut plain = connectors();
        if let Some(Ok(github)) = plain.installed.get_mut("github") {
            for resource in github.surface.resources.values_mut() {
                resource.narrow_when.clear();
            }
        }
        assert!(
            !plain.catalogue().contains("with option"),
            "a definition with no narrow_when is told nothing about it"
        );
    }

    /// An option is one the definition lists, and one the plan left out takes the definition's default,
    /// shown on the line as if the plan had given it.
    #[test]
    fn an_option_is_one_the_definition_lists_and_an_omitted_one_takes_its_default() {
        let review = |option: Option<&str>| {
            let mut step = act("review").with_text("on", "p");
            if let Some(option) = option {
                step = step.with_text("option", option);
            }
            vec![fetch("pull", "p").with_text("item", "7"), step]
        };
        assert!(
            checked(review(Some("DISMISS")))
                .unwrap_err()
                .contains("COMMENT")
        );
        assert!(checked(review(Some("REQUEST_CHANGES"))).is_ok());

        let connectors = connectors();
        let draft = connectors.with_defaults(Draft::new(review(None)));
        let plan = validate_for_tests(&draft).unwrap();
        let routing: BTreeMap<_, _> = plan.routing().into_iter().collect();
        assert_eq!(routing["step_1_option"], "COMMENT");
        assert!(connectors.check(&plan).is_ok());
    }

    /// An act's `on` names a fetch of its own connector and place that chose one item, so it cannot reach
    /// an item from anywhere its locked place does not cover.
    #[test]
    fn an_act_lands_only_on_an_item_a_fetch_chose_and_narrowed() {
        // A listing chooses no single item.
        assert!(
            checked(vec![
                fetch("list", "l").with_text("select", "state:open"),
                act("comment").with_text("on", "l"),
            ])
            .unwrap_err()
            .contains("chooses no single item")
        );
        // A review of "some pull request" picked by state alone is refused, and one narrowed by
        // its branch is not.
        assert!(
            checked(vec![
                fetch("find_pull", "p").with_text("select", "state:open"),
                act("review").with_text("on", "p"),
            ])
            .unwrap_err()
            .contains("narrow")
        );
        assert!(
            checked(vec![
                fetch("find_pull", "p").with_text("select", "state:open head:brave:fix"),
                act("review").with_text("on", "p"),
            ])
            .is_ok()
        );
        // A pull request named by number needs no narrowing.
        assert!(
            checked(vec![
                fetch("pull", "p").with_text("item", "7"),
                act("review").with_text("on", "p"),
            ])
            .is_ok()
        );
    }

    fn reply_plan() -> Vec<DraftStep> {
        vec![
            fetch("find_issue", "i").with_text("select", "state:open"),
            DraftStep::new("process")
                .with(
                    "reads",
                    bravebot_core::manifest::Arg::List(vec!["i".to_string()]),
                )
                .with_text("instruction", "draft an answer")
                .with_text("out_slot", "draft"),
            DraftStep::new(CONNECTOR_ACT)
                .with_text("connector", "github")
                .with_text("resource", "reply")
                .with_text("place", "brave/bravebot")
                .with_text("on", "i")
                .with_text("from_slot", "draft"),
        ]
    }

    /// Values the authority takes from the item an act lands on are never given by the plan, so a plan
    /// cannot address a reply somewhere else.
    #[test]
    fn an_act_that_takes_values_from_its_item_is_planned_without_them() {
        assert!(checked(reply_plan()).is_ok());
        // The plan names a value the authority fills in: refused before anybody is asked.
        let mut with_title = reply_plan();
        with_title[2] = with_title[2].clone().with_text("title", "Anything");
        let refused = checked(with_title).unwrap_err();
        assert!(
            refused.contains("takes 'title' from the item it acts on"),
            "{refused}"
        );
        // It needs the item: no `on`, no plan.
        let mut no_item = reply_plan();
        no_item[2].args.remove("on");
        assert!(checked(no_item).unwrap_err().contains("on"));
        // And the item has to be one a fetch chose, not a listing.
        let mut listing = reply_plan();
        listing[0] = fetch("list", "i").with_text("select", "state:open");
        assert!(
            checked(listing)
                .unwrap_err()
                .contains("chooses no single item")
        );
    }

    /// The line a person approves says what the authority fills in for an act, and from which fetch's
    /// item, so nothing it sends is a surprise.
    #[test]
    fn the_plan_line_says_what_the_authority_fills_in_and_from_which_fetch() {
        let connectors = connectors();
        let draft = connectors.with_defaults(Draft::new(reply_plan()));
        let plan = validate_for_tests(&draft).unwrap();
        connectors.check(&plan).unwrap();
        let notes = connectors.notes(&plan);
        assert_eq!(
            notes.keys().copied().collect::<Vec<_>>(),
            vec![2],
            "only the reply has one"
        );
        let plan = plan.with_notes(notes);
        let line = plan.describe_step(2);
        assert!(
            line.contains("the authority fills in, from the item 'i' chose"),
            "{line}"
        );
        assert!(
            line.contains("recipient = the author of that item"),
            "{line}"
        );
        assert!(line.contains("title = Re: and that item's title"), "{line}");
        // The content leaves only to the place the plan named, as for any act.
        assert!(
            line.contains("content from github brave/bravebot to github brave/bravebot"),
            "{line}"
        );
        assert_eq!(plan.releases()[0].destination(), "github brave/bravebot");
        assert_eq!(
            plan.describe_step(0),
            plan.steps()[0].describe(),
            "a fetch gets no note"
        );
    }

    /// The text and title a plan wrote are shown on its line, on one line with control characters
    /// escaped, so they cannot draw a line of their own.
    #[test]
    fn the_plan_line_shows_the_text_and_title_the_plan_wrote_on_one_line() {
        let connectors = connectors();
        let long = "x".repeat(250);
        let draft = connectors.with_defaults(Draft::new(vec![
            fetch("issue", "i").with_text("item", "4"),
            act("put_file")
                .with_text("item", "bravebot/fix")
                .with_text("path", "docs/x.md")
                .with_text("title", "Fix the docs")
                .with_text("contents", "line one\nline two\u{2028}\u{1b}[2J"),
            act("comment")
                .with_text("item", "4")
                .with_text("contents", &long),
            DraftStep::new(CONNECTOR_ACT)
                .with_text("connector", "github")
                .with_text("resource", "comment")
                .with_text("place", "brave/bravebot")
                .with_text("item", "4")
                .with_text("from_slot", "i"),
        ]));
        let plan = validate_for_tests(&draft).unwrap();
        connectors.check(&plan).unwrap();
        let notes = connectors.notes(&plan);
        assert_eq!(
            notes[&1],
            "sends: “line one\\nline two\\u{2028}\\u{1b}[2J”; title: “Fix the docs”"
        );
        assert_eq!(notes[&2], format!("sends: “{}…”", "x".repeat(200)));
        assert!(
            !notes.contains_key(&3),
            "text a slot carries is not known before the run: {notes:?}"
        );
        let plan = plan.with_notes(notes);
        assert!(
            plan.describe_step(1).ends_with("; title: “Fix the docs”"),
            "{}",
            plan.describe_step(1)
        );
    }

    /// The planner is told which values the authority fills in, so it does not give them.
    #[test]
    fn the_planner_is_told_which_values_it_must_not_give() {
        let shown = connectors().catalogue();
        assert!(
            shown.contains("filled in from the fetch named in on, so never give: recipient (the author of that item); title (Re: and that item's title)"),
            "{shown}"
        );
    }
}
