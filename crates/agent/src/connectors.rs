//! The connector definitions a manifest run may use, and what a plan's connector steps have to
//! satisfy before the plan is put to anybody.
//!
//! Nothing here names a service. A definition's surface says which resources exist, which fields
//! each takes, the form each field's value must have, the values a field may take and the selector
//! terms a fetch accepts, and this compares a plan's text against it. Comparing text a person put
//! in their own directory against text the planner wrote from the task alone decides nothing from
//! anything a service sent.
//!
//! A plan may fetch through a connector and not yet act through one, so only a definition's fetch
//! resources are offered.

use bravebot_config::connectors::{Connector, StepKind, forms};
use bravebot_core::manifest::{Arg, Draft, Manifest, Step, is_connector};
use bravebot_core::selector;
use bravebot_core::value::Labelled;
use std::collections::BTreeMap;
use std::path::Path;

/// The fields every connector step takes whatever its resource, which are not the definition's to
/// list: which connector, which resource, and the slot a fetch fills.
const ALWAYS: [&str; 3] = ["connector", "resource", "out_slot"];

/// What a run knows about connectors: the installed definitions, and what it learnt as it ran.
#[derive(Debug, Default)]
pub struct Connectors {
    installed: BTreeMap<String, Result<Connector, String>>,
    run: String,
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

    /// Keep the detail an authority reported about a fetch it did not accept, for the person to
    /// read.
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

    /// What the goal call is told about the installed connectors: each one's name and what its
    /// fetches read, in the definition's own summaries, and no step or field name, since that call
    /// describes the work and names no tools.
    pub fn in_plain_words(&self) -> String {
        let mut lines = String::from("Connectors:\n");
        for connector in self.usable() {
            let summaries: Vec<&str> = connector
                .surface
                .resources
                .values()
                .filter(|resource| resource.step == StepKind::Fetch)
                .map(|resource| resource.summary.as_str())
                .collect();
            lines.push_str(&format!("  {}: {}\n", connector.name, summaries.join("; ")));
        }
        lines
    }

    /// What the planner is told about the installed connectors, under the capability that fetches.
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
            let fetches = connector
                .surface
                .resources
                .iter()
                .filter(|(_, resource)| resource.step == StepKind::Fetch);
            for (name, resource) in fetches.clone() {
                lines.push_str(&format!(
                    "    {name} (CONNECTOR_FETCH) - {}. uses: {}\n",
                    resource.summary,
                    resource.uses.join(", ")
                ));
                if !resource.select.is_empty() {
                    lines.push_str(&format!(
                        "      select terms: {}\n",
                        resource.select.join(" ")
                    ));
                }
                if !resource.narrow.is_empty() {
                    lines.push_str(&format!(
                        "      select must use one of: {}\n",
                        resource.narrow.join(" ")
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
            // A field's values are told only where a fetch offered here takes that field.
            for (field, values) in &connector.surface.values {
                if !fetches
                    .clone()
                    .any(|(_, resource)| resource.uses_field(field))
                {
                    continue;
                }
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
            for (field, value) in &connector.surface.defaults {
                // A field that is half of an either-or pair is left empty when the other half
                // is given, or the step would hold both.
                let Some(entry) = resource
                    .alternatives()
                    .find(|fields| fields.contains(&field.as_str()))
                else {
                    continue;
                };
                let alternative_given = entry
                    .iter()
                    .any(|f| f != field && !text(&step.args, f).trim().is_empty());
                if !alternative_given && text(&step.args, field).trim().is_empty() {
                    step.args.insert(field.clone(), Arg::Text(value.clone()));
                }
            }
        }
        draft
    }

    /// Hold every connector step in a frozen plan to its definition's surface.
    pub fn check(&self, plan: &Manifest) -> Result<(), String> {
        for (index, step) in plan.steps().iter().enumerate() {
            if !is_connector(step.tool()) {
                continue;
            }
            self.check_step(step)
                .map_err(|why| format!("step {}: {why}", index + 1))?;
        }
        Ok(())
    }

    fn check_step(&self, step: &Step) -> Result<(), String> {
        let connector = self.connector(step.text("connector"))?;
        let surface = &connector.surface;
        let name = step.text("resource");
        let resource = surface
            .resources
            .get(name)
            .ok_or_else(|| format!("'{}' offers no resource '{name}'", connector.name))?;
        if resource.step != StepKind::Fetch {
            return Err(format!("'{name}' is not a resource this step can use"));
        }

        let given = |field: &str| !step.text(field).trim().is_empty();
        for field in ["place", "item", "select"] {
            if given(field) && !resource.uses_field(field) {
                return Err(format!("'{name}' does not take '{field}'"));
            }
        }
        for alternatives in resource.alternatives() {
            let present = alternatives.iter().filter(|f| given(f)).count();
            // A fetch's selector only narrows what it reads, as the authority holds it too.
            let fine = match alternatives.as_slice() {
                [only] => present == 1 || ALWAYS.contains(only) || *only == "select",
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
        if !resource.narrow.is_empty()
            && !terms.iter().any(|(key, value)| {
                resource
                    .narrow
                    .iter()
                    .any(|p| selector::matches(p, key, value))
            })
        {
            return Err(format!(
                "'{name}' has to be narrowed with one of {}",
                resource.narrow.join(" ")
            ));
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

    /// A definition with fetches and an act, so what is not offered yet is in it too.
    fn github() -> Connector {
        connector(
            "github",
            serde_json::json!({
                "resources": {
                    "issue": { "step": "fetch", "uses": ["place", "item"], "summary": "one issue" },
                    "find_issue": { "step": "fetch", "uses": ["place", "select"], "summary": "first issue",
                        "select": ["state:*", "label:*", "no:label", "sort:created-*"], "receipt": true },
                    "find_pull": { "step": "fetch", "uses": ["place", "select"], "summary": "first pull",
                        "select": ["state:*", "head:*", "label:*", "sort:created-*", "sort:updated-*"],
                        "receipt": true },
                    "list": { "step": "fetch", "uses": ["place", "select"], "summary": "many",
                        "select": ["state:*", "sort:comments-*"] },
                    "search": { "step": "fetch", "uses": ["select"], "summary": "across places",
                        "select": ["text:*", "user:*"] },
                    "comment": { "step": "act", "uses": ["place", "item"], "summary": "comment" },
                    "review": { "step": "act", "uses": ["place", "item", "option"], "summary": "review" }
                },
                "values": { "option": ["COMMENT", "REQUEST_CHANGES", "APPROVE"] },
                "defaults": { "option": "COMMENT" },
                "forms": { "place": "owner/name", "item": "positive-integer" }
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

    /// The goal call describes the work and names no tools, so it is told what each connector reads in
    /// the definition's words, and nothing a plan may not yet do.
    #[test]
    fn the_goal_call_is_told_each_connector_and_what_it_fetches_in_plain_words() {
        let told = connectors().in_plain_words();
        assert!(told.starts_with("Connectors:\n  github: "), "{told}");
        assert!(told.contains("one issue"), "{told}");
        assert!(!told.contains("comment"), "an act is not offered: {told}");
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
            checked(vec![fetch("comment", "c").with_text("item", "1")])
                .unwrap_err()
                .contains("not a resource this step can use")
        );
    }

    /// A mail definition goes through the same check with no item form of its own, so a path or a query
    /// cannot pass as a message id.
    #[test]
    fn mail_reads_take_a_selector_or_an_opaque_id_and_nothing_else() {
        let gmail = connector(
            "gmail",
            serde_json::json!({
                "resources": {
                    "inbox": { "step": "fetch", "uses": ["place", "select"], "summary": "recent mail",
                               "forms": { "place": "opaque-id" },
                               "select": ["in:inbox", "is:unread", "from:*", "limit:*"] },
                    "message": { "step": "fetch", "uses": ["place", "item"], "summary": "one email",
                                 "forms": { "place": "opaque-id" } }
                },
                "forms": { "item": "opaque-id" },
                "defaults": { "place": "me" }
            }),
        );
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
    }

    /// The planner is told how to quote a value with a space, and the grammar reads it back as one term.
    #[test]
    fn the_planner_is_told_a_value_with_a_space_is_quoted_and_such_a_term_parses() {
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
        assert!(!connectors.any());
        let told = connectors.refused();
        assert!(
            told.contains("broken: bad json") && told.contains("refused"),
            "{told}"
        );
        assert!(Connectors::default().refused().is_empty());
        assert!(
            Connectors::default()
                .catalogue()
                .contains("Connectors installed")
        );
    }

    /// A plan may fetch and may not yet act, so the planner is offered each fetch resource and none of a
    /// definition's acts, nor a field only an act takes.
    #[test]
    fn the_catalogue_offers_each_fetch_and_no_act() {
        let shown = connectors().catalogue();
        assert!(shown.contains("find_issue (CONNECTOR_FETCH) - first issue. uses: place, select"));
        assert!(
            !shown.contains("comment (") && !shown.contains("review"),
            "{shown}"
        );
        assert!(
            !shown.contains("option is one of"),
            "a field only an act takes is not told: {shown}"
        );
        assert!(
            !shown.contains(CONNECTOR_FETCH),
            "a tool name leaked into the catalogue"
        );
    }
}
