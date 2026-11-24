//! The requests the `web_search` tool makes of the Brave Search API, built from the planner's
//! arguments (`docs/specs/tools/web-search.md`).
//!
//! Nothing here sends anything or reads a reply. Given the arguments a call carried, after the
//! driver has read each string through `Policy::read_planner_argument`, this checks every one
//! against the type, range and set the vendor documents for the service and writes the URL. The
//! host and the path are written in this file: no argument names either, so the only thing a
//! caller can choose is which of the services below is asked and the values of the parameters it
//! documents.
//!
//! A value is refused here with a sentence this crate wrote, naming the argument from the fixed
//! table below and never from what the planner typed (TOOL-4).

use serde_json::{Map, Value};

/// One of the services `web_search` can ask, which fixes the path and the parameters taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    Web,
    News,
    Videos,
    Images,
    LlmContext,
    Suggest,
    Spellcheck,
    Places,
}

impl Service {
    /// The names the `service` argument takes, in the order the schema lists them.
    pub const NAMES: [&'static str; 8] = [
        "web",
        "news",
        "videos",
        "images",
        "llm_context",
        "suggest",
        "spellcheck",
        "places",
    ];

    /// The service a name asks for, or `None` for a name that is not one of [`Service::NAMES`].
    pub fn named(name: &str) -> Option<Self> {
        Some(match name {
            "web" => Self::Web,
            "news" => Self::News,
            "videos" => Self::Videos,
            "images" => Self::Images,
            "llm_context" => Self::LlmContext,
            "suggest" => Self::Suggest,
            "spellcheck" => Self::Spellcheck,
            "places" => Self::Places,
            _ => return None,
        })
    }

    /// The name of the service as the argument spells it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::News => "news",
            Self::Videos => "videos",
            Self::Images => "images",
            Self::LlmContext => "llm_context",
            Self::Suggest => "suggest",
            Self::Spellcheck => "spellcheck",
            Self::Places => "places",
        }
    }

    /// The path under the base, written here and nowhere else.
    fn path(self) -> &'static str {
        match self {
            Self::Web => "/v1/web/search",
            Self::News => "/v1/news/search",
            Self::Videos => "/v1/videos/search",
            Self::Images => "/v1/images/search",
            Self::LlmContext => "/v1/llm/context",
            Self::Suggest => "/v1/suggest/search",
            Self::Spellcheck => "/v1/spellcheck/search",
            Self::Places => "/v1/local/place_search",
        }
    }

    /// The most characters and words the vendor documents for `q` on this service.
    fn query_limit(self) -> (usize, usize) {
        match self {
            Self::Web | Self::LlmContext => (600, 75),
            _ => (400, 50),
        }
    }

    /// The parameters this service documents, which is the whole of what a call to it may carry
    /// besides `service` and `q`.
    fn takes(self) -> &'static [&'static str] {
        match self {
            Self::Web => &[
                "country",
                "search_lang",
                "ui_lang",
                "count",
                "offset",
                "safesearch",
                "spellcheck",
                "freshness",
                "text_decorations",
                "result_filter",
                "units",
                "extra_snippets",
                "operators",
                "operators_fallback",
                "include_fetch_metadata",
            ],
            Self::News => &[
                "search_lang",
                "ui_lang",
                "country",
                "safesearch",
                "count",
                "offset",
                "spellcheck",
                "freshness",
                "extra_snippets",
                "include_fetch_metadata",
                "operators",
                "operators_fallback",
            ],
            Self::Videos => &[
                "search_lang",
                "ui_lang",
                "country",
                "safesearch",
                "count",
                "offset",
                "spellcheck",
                "freshness",
                "include_fetch_metadata",
                "operators",
                "operators_fallback",
            ],
            Self::Images => &[
                "search_lang",
                "country",
                "safesearch",
                "count",
                "spellcheck",
            ],
            Self::LlmContext => &[
                "country",
                "search_lang",
                "count",
                "safesearch",
                "spellcheck",
                "operators",
                "operators_fallback",
                "maximum_number_of_urls",
                "maximum_number_of_tokens",
                "context_threshold_mode",
                "maximum_number_of_tokens_per_url",
                "freshness",
            ],
            Self::Suggest => &["search_lang", "country", "count"],
            Self::Spellcheck => &["search_lang", "country"],
            Self::Places => &[
                "latitude",
                "longitude",
                "location",
                "radius",
                "count",
                "country",
                "search_lang",
                "ui_lang",
                "units",
                "safesearch",
                "spellcheck",
            ],
        }
    }

    /// The largest `count` the vendor documents for this service.
    fn most_results(self) -> u64 {
        match self {
            Self::Web | Self::Suggest => 20,
            Self::News | Self::Videos | Self::LlmContext => 50,
            Self::Places => 100,
            Self::Images => 200,
            Self::Spellcheck => 0,
        }
    }

    /// The name the vendor's parameter has on this service.
    fn wire(self, name: &'static str) -> &'static str {
        match (self, name) {
            (Self::Suggest | Self::Spellcheck, "search_lang") => "lang",
            _ => name,
        }
    }
}

/// How an argument's value is checked.
#[derive(Debug, Clone, Copy)]
enum Kind {
    /// One of a fixed set of words.
    Word(&'static [&'static str]),
    /// A whole number in a closed range.
    Whole(u64, u64),
    /// A JSON boolean.
    Flag,
    /// A preset, or two dates joined by `to`.
    Freshness,
    /// A non-empty comma-separated list of words from a fixed set, as the vendor spells it.
    Words(&'static [&'static str]),
    /// A decimal number in a closed range.
    Degrees(f64, f64),
    /// Free text of at most this many characters, with no control character.
    Text(usize),
}

/// Every parameter of every service, in the order they are written to the URL.
const PARAMETERS: [&str; 23] = [
    "country",
    "search_lang",
    "ui_lang",
    "latitude",
    "longitude",
    "location",
    "radius",
    "count",
    "offset",
    "safesearch",
    "spellcheck",
    "freshness",
    "text_decorations",
    "result_filter",
    "units",
    "extra_snippets",
    "operators",
    "operators_fallback",
    "include_fetch_metadata",
    "maximum_number_of_urls",
    "maximum_number_of_tokens",
    "context_threshold_mode",
    "maximum_number_of_tokens_per_url",
];

const COUNTRIES: &[&str] = &[
    "AR", "AU", "AT", "BE", "BR", "CA", "CL", "DK", "FI", "FR", "DE", "GR", "HK", "IN", "ID", "IT",
    "JP", "KR", "MY", "MX", "NL", "NZ", "NO", "CN", "PL", "PT", "PH", "RU", "SA", "ZA", "ES", "SE",
    "CH", "TW", "TR", "GB", "US", "ALL",
];

const SEARCH_LANGUAGES: &[&str] = &[
    "ar", "eu", "bn", "bg", "ca", "zh-hans", "zh-hant", "hr", "cs", "da", "nl", "en", "en-gb",
    "et", "fi", "fr", "gl", "de", "el", "gu", "he", "hi", "hu", "is", "it", "ja", "jp", "kn", "ko",
    "lv", "lt", "ms", "ml", "mr", "nb", "pl", "pt-br", "pt-pt", "pa", "ro", "ru", "sr", "sk", "sl",
    "es", "sv", "ta", "te", "th", "tr", "uk", "vi",
];

const UI_LANGUAGES: &[&str] = &[
    "es-AR", "en-AU", "de-AT", "nl-BE", "fr-BE", "pt-BR", "en-CA", "fr-CA", "es-CL", "da-DK",
    "fi-FI", "fr-FR", "de-DE", "el-GR", "zh-HK", "en-IN", "en-ID", "it-IT", "ja-JP", "ko-KR",
    "en-MY", "es-MX", "nl-NL", "en-NZ", "no-NO", "zh-CN", "pl-PL", "en-PH", "ru-RU", "en-ZA",
    "es-ES", "sv-SE", "fr-CH", "de-CH", "zh-TW", "tr-TR", "en-GB", "en-US", "es-US",
];

const RESULT_TYPES: &[&str] = &[
    "discussions",
    "faq",
    "infobox",
    "news",
    "query",
    "summarizer",
    "videos",
    "web",
    "locations",
];

/// How the value of `name` on `service` is checked. `None` for a name no service documents.
fn kind_of(service: Service, name: &str) -> Option<Kind> {
    Some(match name {
        "country" => Kind::Word(COUNTRIES),
        "search_lang" => Kind::Word(SEARCH_LANGUAGES),
        "ui_lang" => Kind::Word(UI_LANGUAGES),
        "count" => Kind::Whole(1, service.most_results()),
        "offset" => Kind::Whole(0, 9),
        "safesearch" => match service {
            // Image search takes no `moderate`.
            Service::Images => Kind::Word(&["off", "strict"]),
            _ => Kind::Word(&["off", "moderate", "strict"]),
        },
        "spellcheck"
        | "text_decorations"
        | "extra_snippets"
        | "operators"
        | "operators_fallback"
        | "include_fetch_metadata" => Kind::Flag,
        "freshness" => Kind::Freshness,
        "result_filter" => Kind::Words(RESULT_TYPES),
        "units" => Kind::Word(&["metric", "imperial"]),
        "maximum_number_of_urls" => Kind::Whole(1, 50),
        "maximum_number_of_tokens" => Kind::Whole(1024, 32768),
        "context_threshold_mode" => Kind::Word(&["disabled", "strict", "lenient", "balanced"]),
        "maximum_number_of_tokens_per_url" => Kind::Whole(512, 8192),
        "latitude" => Kind::Degrees(-90.0, 90.0),
        "longitude" => Kind::Degrees(-180.0, 180.0),
        "location" => Kind::Text(200),
        "radius" => Kind::Whole(0, 40_075_000),
        _ => return None,
    })
}

/// A request ready to send: the whole URL, with no credential in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    pub service: Service,
    pub url: String,
}

/// Why a call was not made, in words this repository wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// No `service`, or one that is not a string.
    NoService,
    /// A `service` that is not one of [`Service::NAMES`].
    UnknownService,
    /// No `q`, or one that is not a string.
    NoQuery,
    /// A `q` the service cannot take: empty, too long, too many words or holding a control
    /// character.
    Query(Service, &'static str),
    /// A value outside the type, range or set the vendor documents for the argument.
    Argument(&'static str, String),
    /// An argument another service documents and this one does not.
    NotTaken(&'static str, Service),
    /// A name that is no argument of any service.
    Unknown,
    /// Two arguments that the vendor documents as alternatives, or that only mean something
    /// together.
    Together(&'static str),
}

impl Refusal {
    /// The sentence the planner is told.
    pub fn words(&self) -> String {
        match self {
            Self::NoService => format!(
                "error: web_search needs 'service', one of {}",
                Service::NAMES.join(", ")
            ),
            Self::UnknownService => format!(
                "error: web_search 'service' must be one of {}",
                Service::NAMES.join(", ")
            ),
            Self::NoQuery => "error: web_search needs 'q', the query as a string".to_string(),
            Self::Query(service, why) => format!(
                "error: web_search 'q' for the {} service {why}",
                service.name()
            ),
            Self::Argument(name, expected) => {
                format!("error: web_search '{name}' must be {expected}")
            }
            Self::NotTaken(name, service) => format!(
                "error: the {} service takes no '{name}'. It takes: q{}",
                service.name(),
                service
                    .takes()
                    .iter()
                    .map(|taken| format!(", {taken}"))
                    .collect::<String>()
            ),
            Self::Unknown => "error: web_search was given an argument it does not take".to_string(),
            Self::Together(what) => format!("error: web_search {what}"),
        }
    }
}

/// The URL for a call, or the reason it is refused.
///
/// `arguments` holds every argument the call carried except `why` and `read`, with each string
/// already read through the policy's gate for the planner's own words. `base` is the compiled-in
/// base of the API ([`bravebot_config::WEB_SEARCH_BASE`]) unless a test replaced it.
pub fn prepare(arguments: &Map<String, Value>, base: &str) -> Result<Prepared, Refusal> {
    let service = match arguments.get("service") {
        Some(Value::String(name)) => Service::named(name.trim()).ok_or(Refusal::UnknownService)?,
        _ => return Err(Refusal::NoService),
    };
    let query = match arguments.get("q") {
        Some(Value::String(query)) => Some(query.as_str()),
        Some(_) => return Err(Refusal::NoQuery),
        // Place search lists the points of interest of an area when it is given no query.
        None if service == Service::Places => None,
        None => return Err(Refusal::NoQuery),
    };

    for name in arguments.keys() {
        if name == "service" || name == "q" {
            continue;
        }
        let Some(known) = PARAMETERS.iter().find(|known| *known == name) else {
            return Err(Refusal::Unknown);
        };
        if !service.takes().contains(known) {
            return Err(Refusal::NotTaken(known, service));
        }
    }

    let mut pairs: Vec<(&'static str, String)> = Vec::new();
    if let Some(query) = query {
        pairs.push(("q", checked_query(service, query)?));
    }
    for name in PARAMETERS {
        let Some(value) = arguments.get(name) else {
            continue;
        };
        let Some(kind) = kind_of(service, name) else {
            continue;
        };
        pairs.push((service.wire(name), checked(name, kind, value)?));
    }

    if arguments.contains_key("latitude") != arguments.contains_key("longitude") {
        return Err(Refusal::Together(
            "takes 'latitude' and 'longitude' together or neither",
        ));
    }
    if arguments.contains_key("location") && arguments.contains_key("latitude") {
        return Err(Refusal::Together(
            "takes 'location' or 'latitude' with 'longitude', not both",
        ));
    }

    let mut url = format!("{}{}", base.trim_end_matches('/'), service.path());
    for (position, (name, value)) in pairs.iter().enumerate() {
        url.push(if position == 0 { '?' } else { '&' });
        url.push_str(name);
        url.push('=');
        url.push_str(&encoded(value));
    }
    Ok(Prepared { service, url })
}

/// `q` as the service takes it.
fn checked_query(service: Service, query: &str) -> Result<String, Refusal> {
    let query = query.trim();
    let (characters, words) = service.query_limit();
    if query.is_empty() {
        return Err(Refusal::Query(service, "must not be empty"));
    }
    if query.chars().any(char::is_control) {
        return Err(Refusal::Query(service, "must not hold a control character"));
    }
    if query.chars().count() > characters {
        return Err(Refusal::Query(
            service,
            match characters {
                600 => "must be at most 600 characters",
                _ => "must be at most 400 characters",
            },
        ));
    }
    if query.split_whitespace().count() > words {
        return Err(Refusal::Query(
            service,
            match words {
                75 => "must be at most 75 words",
                _ => "must be at most 50 words",
            },
        ));
    }
    Ok(query.to_string())
}

/// One argument's value, as text for the URL, or the refusal that names it.
fn checked(name: &'static str, kind: Kind, value: &Value) -> Result<String, Refusal> {
    let wrong = |expected: String| Refusal::Argument(name, expected);
    match kind {
        Kind::Word(set) => match value.as_str().map(str::trim) {
            Some(word) if set.contains(&word) => Ok(word.to_string()),
            _ => Err(wrong(format!("one of {}", set.join(", ")))),
        },
        Kind::Whole(least, most) => match value.as_u64() {
            Some(number) if (least..=most).contains(&number) => Ok(number.to_string()),
            _ => Err(wrong(format!(
                "a whole number from {least} to {most} for this service"
            ))),
        },
        Kind::Flag => match value {
            Value::Bool(flag) => Ok(flag.to_string()),
            _ => Err(wrong("true or false".to_string())),
        },
        Kind::Freshness => match value.as_str().map(str::trim) {
            Some(word) if freshness_is_valid(word) => Ok(word.to_string()),
            _ => Err(wrong(
                "pd, pw, pm, py, or two dates as YYYY-MM-DDtoYYYY-MM-DD, the first no later than \
                 the second"
                    .to_string(),
            )),
        },
        Kind::Words(set) => {
            let listed: Option<Vec<&str>> = value.as_str().and_then(|text| {
                text.split(',')
                    .map(|word| Some(word.trim()).filter(|word| set.contains(word)))
                    .collect()
            });
            match listed {
                Some(words) => Ok(words.join(",")),
                None => Err(wrong(format!(
                    "a comma-separated list of words from {}",
                    set.join(", ")
                ))),
            }
        }
        Kind::Degrees(least, most) => match value.as_f64() {
            Some(number) if number >= least && number <= most => Ok(number.to_string()),
            _ => Err(wrong(format!("a number from {least} to {most}"))),
        },
        Kind::Text(longest) => match value.as_str().map(str::trim) {
            Some(text)
                if !text.is_empty()
                    && text.chars().count() <= longest
                    && !text.chars().any(char::is_control) =>
            {
                Ok(text.to_string())
            }
            _ => Err(wrong(format!(
                "text of at most {longest} characters with no control character"
            ))),
        },
    }
}

/// Whether `word` is one of the four presets or `YYYY-MM-DDtoYYYY-MM-DD` with real-looking dates in
/// order.
fn freshness_is_valid(word: &str) -> bool {
    if ["pd", "pw", "pm", "py"].contains(&word) {
        return true;
    }
    let Some((from, until)) = word.split_once("to") else {
        return false;
    };
    let date = |text: &str| -> Option<(u32, u32, u32)> {
        let parts: Vec<&str> = text.split('-').collect();
        let [year, month, day] = parts.as_slice() else {
            return None;
        };
        let digits = |part: &str, length: usize| {
            part.len() == length && part.bytes().all(|byte| byte.is_ascii_digit())
        };
        if !(digits(year, 4) && digits(month, 2) && digits(day, 2)) {
            return None;
        }
        let (year, month, day) = (year.parse().ok()?, month.parse().ok()?, day.parse().ok()?);
        ((1..=12).contains(&month) && (1..=31).contains(&day)).then_some((year, month, day))
    };
    matches!((date(from), date(until)), (Some(from), Some(until)) if from <= until)
}

/// Percent-encoding for one query component: unreserved characters stay and every other byte is
/// written as `%XX`.
fn encoded(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const BASE: &str = "https://api.search.brave.com/res";

    fn arguments(value: Value) -> Map<String, Value> {
        value.as_object().cloned().expect("an object")
    }

    fn url_of(value: Value) -> String {
        prepare(&arguments(value), BASE).expect("accepted").url
    }

    fn refused(value: Value) -> Refusal {
        prepare(&arguments(value), BASE).expect_err("refused")
    }

    /// Each service is asked at its own path under the fixed base, with the query encoded. The
    /// regression it rejects: a path taken from an argument, or two services sharing one path.
    #[test]
    fn each_service_has_its_own_path_under_the_fixed_base() {
        for (service, path) in [
            ("web", "/v1/web/search"),
            ("news", "/v1/news/search"),
            ("videos", "/v1/videos/search"),
            ("images", "/v1/images/search"),
            ("llm_context", "/v1/llm/context"),
            ("suggest", "/v1/suggest/search"),
            ("spellcheck", "/v1/spellcheck/search"),
            ("places", "/v1/local/place_search"),
        ] {
            assert_eq!(
                url_of(json!({"service": service, "q": "rust async"})),
                format!("{BASE}{path}?q=rust%20async"),
                "{service}"
            );
        }
    }

    /// A query is one value of one parameter, so a character that ends it is encoded and cannot add
    /// a parameter or a fragment. The regression it rejects: the query written into the URL as
    /// typed.
    #[test]
    fn a_query_cannot_add_a_parameter_or_end_the_url() {
        let url = url_of(json!({"service": "web", "q": "a&count=1#x?y=é"}));
        assert_eq!(
            url,
            format!("{BASE}/v1/web/search?q=a%26count%3D1%23x%3Fy%3D%C3%A9")
        );
    }

    /// Parameters are written in a fixed order whatever order the call listed them in, and under the
    /// vendor's names. The regression it rejects: `search_lang` sent to suggest and spellcheck,
    /// which document `lang`.
    #[test]
    fn parameters_keep_the_vendors_names_and_a_fixed_order() {
        assert_eq!(
            url_of(json!({"service": "web", "q": "x", "count": 5, "country": "GB", "offset": 2})),
            format!("{BASE}/v1/web/search?q=x&country=GB&count=5&offset=2")
        );
        assert_eq!(
            url_of(json!({"service": "suggest", "q": "x", "search_lang": "fr"})),
            format!("{BASE}/v1/suggest/search?q=x&lang=fr")
        );
        assert_eq!(
            url_of(json!({"service": "spellcheck", "q": "x", "search_lang": "fr"})),
            format!("{BASE}/v1/spellcheck/search?q=x&lang=fr")
        );
        assert_eq!(
            url_of(json!({"service": "web", "q": "x", "search_lang": "fr"})),
            format!("{BASE}/v1/web/search?q=x&search_lang=fr")
        );
    }

    /// The number of results has a ceiling per service. The regression it rejects: one ceiling for
    /// every service, which sends the web service a count it answers with a 422 and refuses image
    /// search a count it takes.
    #[test]
    fn count_is_bounded_by_the_service_asked() {
        for (service, most) in [
            ("web", 20),
            ("news", 50),
            ("videos", 50),
            ("images", 200),
            ("llm_context", 50),
            ("suggest", 20),
            ("places", 100),
        ] {
            let ask = |count: u64| json!({"service": service, "q": "x", "count": count});
            assert!(
                prepare(&arguments(ask(most)), BASE).is_ok(),
                "{service} {most}"
            );
            assert_eq!(
                refused(ask(most + 1)),
                Refusal::Argument(
                    "count",
                    format!("a whole number from 1 to {most} for this service")
                ),
                "{service}"
            );
            assert!(prepare(&arguments(ask(0)), BASE).is_err(), "{service} 0");
        }
    }

    /// An argument one service documents is refused on another, by name, rather than sent for the
    /// vendor to answer. The regression it rejects: every argument forwarded to every service.
    #[test]
    fn an_argument_the_service_does_not_document_is_refused() {
        assert_eq!(
            refused(json!({"service": "images", "q": "x", "offset": 1})),
            Refusal::NotTaken("offset", Service::Images)
        );
        assert_eq!(
            refused(json!({"service": "web", "q": "x", "maximum_number_of_urls": 3})),
            Refusal::NotTaken("maximum_number_of_urls", Service::Web)
        );
        assert_eq!(
            refused(json!({"service": "spellcheck", "q": "x", "count": 3})),
            Refusal::NotTaken("count", Service::Spellcheck)
        );
        let said = Refusal::NotTaken("offset", Service::Images).words();
        assert!(
            said.contains("'offset'") && said.contains("images"),
            "{said}"
        );
    }

    /// Image search takes `off` and `strict` and no `moderate`, and the other services take all
    /// three. The regression it rejects: one set of safesearch words for every service.
    #[test]
    fn image_search_takes_no_moderate_safesearch() {
        assert!(
            prepare(
                &arguments(json!({"service": "images", "q": "x", "safesearch": "strict"})),
                BASE
            )
            .is_ok()
        );
        assert_eq!(
            refused(json!({"service": "images", "q": "x", "safesearch": "moderate"})),
            Refusal::Argument("safesearch", "one of off, strict".to_string())
        );
        assert!(
            prepare(
                &arguments(json!({"service": "web", "q": "x", "safesearch": "moderate"})),
                BASE
            )
            .is_ok()
        );
    }

    /// The query limits are the vendor's per service: 600 characters and 75 words for web and LLM
    /// context, 400 and 50 for the rest, and an empty query is refused. The regression it rejects:
    /// one limit for all, or none.
    #[test]
    fn a_query_is_held_to_its_services_limits() {
        let long = "a".repeat(601);
        assert_eq!(
            refused(json!({"service": "web", "q": long})),
            Refusal::Query(Service::Web, "must be at most 600 characters")
        );
        let four_hundred_and_one = "a".repeat(401);
        assert!(
            prepare(
                &arguments(json!({"service": "web", "q": four_hundred_and_one.clone()})),
                BASE
            )
            .is_ok()
        );
        assert_eq!(
            refused(json!({"service": "news", "q": four_hundred_and_one})),
            Refusal::Query(Service::News, "must be at most 400 characters")
        );
        let seventy_six = vec!["w"; 76].join(" ");
        assert_eq!(
            refused(json!({"service": "web", "q": seventy_six})),
            Refusal::Query(Service::Web, "must be at most 75 words")
        );
        let fifty_one = vec!["w"; 51].join(" ");
        assert_eq!(
            refused(json!({"service": "suggest", "q": fifty_one})),
            Refusal::Query(Service::Suggest, "must be at most 50 words")
        );
        assert_eq!(
            refused(json!({"service": "web", "q": "   "})),
            Refusal::Query(Service::Web, "must not be empty")
        );
        assert_eq!(refused(json!({"service": "web"})), Refusal::NoQuery);
        assert_eq!(refused(json!({"service": "web", "q": 5})), Refusal::NoQuery);
    }

    /// Place search lists an area's points of interest without a query, and every other service
    /// needs one. The regression it rejects: a query required of place search, or optional
    /// elsewhere.
    #[test]
    fn only_place_search_may_go_without_a_query() {
        assert_eq!(
            url_of(json!({"service": "places", "latitude": 37.77, "longitude": -122.42})),
            format!("{BASE}/v1/local/place_search?latitude=37.77&longitude=-122.42")
        );
        assert_eq!(
            refused(json!({"service": "news", "count": 3})),
            Refusal::NoQuery
        );
    }

    /// A place is a pair of coordinates or a location text, and a coordinate alone means nothing.
    /// The regression it rejects: one coordinate sent, or both ways to name the place sent.
    #[test]
    fn a_place_is_named_one_way_and_coordinates_come_as_a_pair() {
        assert!(matches!(
            refused(json!({"service": "places", "latitude": 1.0})),
            Refusal::Together(_)
        ));
        assert!(matches!(
            refused(json!({"service": "places", "longitude": 1.0})),
            Refusal::Together(_)
        ));
        assert!(matches!(
            refused(
                json!({"service": "places", "latitude": 1.0, "longitude": 1.0, "location": "tokyo japan"})
            ),
            Refusal::Together(_)
        ));
        assert_eq!(
            url_of(json!({"service": "places", "location": "tokyo japan", "radius": 500})),
            format!("{BASE}/v1/local/place_search?location=tokyo%20japan&radius=500")
        );
        assert!(matches!(
            refused(json!({"service": "places", "latitude": 91.0, "longitude": 1.0})),
            Refusal::Argument("latitude", _)
        ));
        assert!(matches!(
            refused(json!({"service": "places", "latitude": 1.0, "longitude": 181.0})),
            Refusal::Argument("longitude", _)
        ));
    }

    /// Freshness is a preset or two dates in order. The regression it rejects: any string passed on,
    /// or a range whose end is before its start.
    #[test]
    fn freshness_is_a_preset_or_an_ordered_range_of_dates() {
        for fresh in [
            "pd",
            "pw",
            "pm",
            "py",
            "2022-04-01to2022-07-30",
            "2022-04-01to2022-04-01",
        ] {
            assert!(
                prepare(
                    &arguments(json!({"service": "web", "q": "x", "freshness": fresh})),
                    BASE
                )
                .is_ok(),
                "{fresh}"
            );
        }
        for bad in [
            "",
            "day",
            "2022-07-30to2022-04-01",
            "2022-13-01to2022-14-01",
            "2022-4-1to2022-7-30",
            "2022-04-01",
            "2022-04-01to",
            "pdtopd",
        ] {
            assert!(
                prepare(
                    &arguments(json!({"service": "web", "q": "x", "freshness": bad})),
                    BASE
                )
                .is_err(),
                "{bad:?} was accepted"
            );
        }
    }

    /// The result filter is a comma-separated string of the documented result types, as the vendor
    /// spells it. The regression it rejects: an empty entry, a word off the list, or a non-string
    /// accepted.
    #[test]
    fn a_result_filter_is_a_list_of_documented_result_types() {
        assert_eq!(
            url_of(json!({"service": "web", "q": "x", "result_filter": "web, news"})),
            format!("{BASE}/v1/web/search?q=x&result_filter=web%2Cnews")
        );
        for bad in [
            json!(""),
            json!("web,"),
            json!("web,images"),
            json!(["web"]),
            json!(1),
        ] {
            assert!(
                prepare(
                    &arguments(json!({"service": "web", "q": "x", "result_filter": bad})),
                    BASE
                )
                .is_err()
            );
        }
    }

    /// Whole numbers, flags and words are held to their types, so a number sent as text and a flag
    /// sent as a word are refused with the argument named. The regression it rejects: coercing
    /// types, which turns `"5"` into a count and `"yes"` into a flag.
    #[test]
    fn types_are_not_coerced() {
        for (name, bad) in [
            ("count", json!("5")),
            ("count", json!(5.5)),
            ("offset", json!(10)),
            ("spellcheck", json!("true")),
            ("spellcheck", json!(1)),
            ("country", json!("us")),
            ("country", json!("ZZ")),
            ("search_lang", json!("EN")),
            ("ui_lang", json!("en")),
            ("units", json!("miles")),
        ] {
            let mut call = json!({"service": "web", "q": "x"});
            call[name] = bad.clone();
            assert!(
                matches!(refused(call), Refusal::Argument(refused_name, _) if refused_name == name),
                "{name} {bad}"
            );
        }
    }

    /// The LLM context limits are the vendor's. The regression it rejects: a token budget outside
    /// the documented range sent for the vendor to refuse.
    #[test]
    fn llm_context_limits_are_the_documented_ranges() {
        let ask = |name: &str, value: u64| {
            let mut call = json!({"service": "llm_context", "q": "x"});
            call[name] = json!(value);
            prepare(&arguments(call), BASE)
        };
        for (name, least, most) in [
            ("maximum_number_of_urls", 1, 50),
            ("maximum_number_of_tokens", 1024, 32768),
            ("maximum_number_of_tokens_per_url", 512, 8192),
        ] {
            assert!(
                ask(name, least).is_ok() && ask(name, most).is_ok(),
                "{name}"
            );
            assert!(
                ask(name, least - 1).is_err() && ask(name, most + 1).is_err(),
                "{name}"
            );
        }
        assert_eq!(
            url_of(
                json!({"service": "llm_context", "q": "x", "context_threshold_mode": "balanced"})
            ),
            format!("{BASE}/v1/llm/context?q=x&context_threshold_mode=balanced")
        );
    }

    /// No argument can name a host, a path, a URL or a goggle, so a request always goes to the base
    /// the driver was given. The regression it rejects: a pass-through of unknown arguments.
    #[test]
    fn no_argument_names_a_host_a_path_or_a_goggle() {
        for name in [
            "url",
            "host",
            "path",
            "base",
            "endpoint",
            "goggles",
            "goggles_id",
            "api_key",
        ] {
            let mut call = json!({"service": "web", "q": "x"});
            call[name] = json!("https://elsewhere.test/");
            assert_eq!(refused(call), Refusal::Unknown, "{name}");
        }
        let url = url_of(json!({"service": "web", "q": "https://elsewhere.test/"}));
        assert!(
            url.starts_with(&format!("{BASE}/v1/web/search?q=")),
            "{url}"
        );
    }

    /// A service that is not one of the set is refused, and so is a call with none. The regression
    /// it rejects: a default service chosen for a call that named none.
    #[test]
    fn a_missing_or_unknown_service_is_refused() {
        assert_eq!(refused(json!({"q": "x"})), Refusal::NoService);
        assert_eq!(refused(json!({"service": 3, "q": "x"})), Refusal::NoService);
        assert_eq!(
            refused(json!({"service": "answers", "q": "x"})),
            Refusal::UnknownService
        );
        assert_eq!(
            refused(json!({"service": "../web", "q": "x"})),
            Refusal::UnknownService
        );
    }

    /// The words of a refusal come from this file's own table. The regression it rejects: a
    /// refusal that repeats what the planner typed.
    #[test]
    fn a_refusal_repeats_nothing_the_planner_typed() {
        let marker = "SENTINEL-WORDS-FROM-THE-PLANNER";
        for call in [
            json!({"service": marker, "q": "x"}),
            json!({"service": "web", "q": "x", marker: 1}),
            json!({"service": "web", "q": "x", "country": marker}),
            json!({"service": "web", "q": "x", "freshness": marker}),
            json!({"service": "web", "q": "x", "count": marker}),
            json!({"service": "places", "location": marker.repeat(10)}),
        ] {
            let said = refused(call).words();
            assert!(!said.contains(marker), "{said}");
        }
    }
}
