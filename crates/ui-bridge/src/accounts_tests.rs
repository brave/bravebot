use super::*;
use bravebot_config::{Settings, env_var};

fn configured(settings: &str) -> Result<Config, ConfigError> {
    let mut config = Config::from_lookup(|name| match name {
        env_var::USE_BEDROCK => Some("1".into()),
        env_var::AWS_REGION => Some("us-east-1".into()),
        _ => None,
    })
    .expect("a configuration");
    config.providers = Settings::parse(settings).providers().to_vec();
    Ok(config)
}

fn before(writable: bool, config: Result<Config, ConfigError>, stored: bool) -> Before {
    Before {
        writable,
        config,
        stored,
    }
}

fn not_signed_in(why: Why) -> Standing {
    Standing::NotSignedIn(why)
}

#[test]
fn a_command_is_given_where_signing_in_is_the_remedy() {
    let missing = || not_signed_in(Why::NothingStored);
    let cases = [
        (Way::Brave, "bravebot auth login brave"),
        (
            Way::Bedrock { profile: None },
            "bravebot auth login bedrock",
        ),
        (
            Way::Gateway {
                id: "open-router_2.x".into(),
            },
            "bravebot auth login gateway open-router_2.x",
        ),
        (Way::Gateways, "bravebot auth login gateway"),
        (Way::ChatGpt, "bravebot auth login chatgpt"),
        (Way::Xai, "bravebot auth login xai"),
    ];
    for (way, said) in cases {
        assert_eq!(command(&way, &missing()).as_deref(), Some(said), "{way:?}");
    }
    assert_eq!(
        command(
            &Way::Bedrock { profile: None },
            &not_signed_in(Why::BedrockLapsed)
        )
        .as_deref(),
        Some("bravebot auth login bedrock")
    );
}

#[test]
fn no_command_is_given_where_signing_in_would_not_help() {
    assert_eq!(
        command(&Way::Brave, &Standing::SignedIn(Held::KeyStored)),
        None
    );
    assert_eq!(command(&Way::Brave, &not_signed_in(Why::Nowhere)), None);
    assert_eq!(
        command(&Way::Gateways, &not_signed_in(Why::GatewayNoneConfigured)),
        None
    );
    for why in [
        Why::BedrockNoCli,
        Why::BedrockNoProfile,
        Why::BedrockTimedOut,
    ] {
        assert_eq!(
            command(&Way::Bedrock { profile: None }, &Standing::Unusable(why)),
            None
        );
    }
    assert_eq!(
        command(
            &Way::Brave,
            &Standing::Misconfigured(ConfigError::Missing("X"))
        ),
        None
    );
}

#[test]
fn no_command_names_an_id_a_shell_would_not_read_as_typed() {
    for id in ["", "a b", "a;rm", "$(x)", "a`b`", "ü", "a/b", "'q'"] {
        assert!(!typed_as_is(id), "{id:?}");
        assert_eq!(
            command(
                &Way::Gateway { id: id.into() },
                &not_signed_in(Why::GatewayAbsent)
            ),
            None,
            "{id:?}"
        );
    }
    assert!(typed_as_is("Open-Router_1.0"));
}

#[test]
fn the_block_hides_rows_about_nothing_to_sign_in_to() {
    let row = |way, standing| Row {
        way,
        standing,
        stored: false,
    };
    assert!(!shown(&row(
        Way::Gateways,
        not_signed_in(Why::NothingStored)
    )));
    assert!(shown(&row(
        Way::Gateways,
        Standing::Unusable(Why::KeysUnreadable)
    )));
    assert!(!shown(&row(
        Way::Bedrock { profile: None },
        not_signed_in(Why::NoAwsAccount)
    )));
    assert!(shown(&row(
        Way::Bedrock { profile: None },
        not_signed_in(Why::BedrockLapsed)
    )));
    assert!(shown(&row(Way::Xai, not_signed_in(Why::NothingStored))));
}

#[test]
fn only_what_bravebot_keeps_can_be_signed_out_of() {
    let row = |way, stored| Row {
        way,
        standing: Standing::SignedIn(Held::KeyStored),
        stored,
    };
    assert!(can_sign_out(&row(Way::Brave, true)));
    assert!(can_sign_out(&row(Way::Gateway { id: "g".into() }, true)));
    assert!(can_sign_out(&row(Way::ChatGpt, true)));
    assert!(can_sign_out(&row(Way::Xai, true)));
    assert!(!can_sign_out(&row(Way::Xai, false)));
    assert!(!can_sign_out(&row(Way::Bedrock { profile: None }, true)));
    assert!(!can_sign_out(&row(Way::Gateways, true)));
}

#[test]
fn a_signed_in_row_or_one_with_nothing_stored_has_no_sentence() {
    assert_eq!(detail(&Standing::SignedIn(Held::AwsSession)), None);
    for why in [
        Why::NothingStored,
        Why::GatewayNoneStored,
        Why::NoAwsAccount,
    ] {
        assert_eq!(detail(&not_signed_in(why)), None);
    }
    for why in [
        Why::Nowhere,
        Why::BraveUnreadable {
            path: "/home/a/.bravebot/brave-key.json".into(),
        },
        Why::BedrockLapsed,
        Why::KeysUnreadable,
        Why::NotChecked,
    ] {
        let said = detail(&Standing::Unusable(why)).expect("a sentence");
        assert!(!said.is_empty());
        assert!(!said.contains("brave-key.json"), "{said}");
    }
    assert!(detail(&Standing::Misconfigured(ConfigError::Missing("X"))).is_some());
}

#[test]
fn a_sign_in_is_refused_in_order_before_it_starts() {
    let plain = || configured("{}");
    for account in [Account::ChatGpt, Account::Xai] {
        assert_eq!(
            refusal(account, &before(false, plain(), true), false),
            Some(t!(desktop_accounts_sign_in_nowhere))
        );
        assert_eq!(
            refusal(
                account,
                &before(true, Err(ConfigError::Missing("X")), true),
                false
            ),
            Some(t!(desktop_accounts_sign_in_misconfigured))
        );
        assert_eq!(
            refusal(account, &before(true, plain(), true), false),
            Some(t!(desktop_accounts_sign_in_stored))
        );
        assert_eq!(refusal(account, &before(true, plain(), true), true), None);
        assert_eq!(refusal(account, &before(true, plain(), false), false), None);
    }
}

#[test]
fn a_sign_in_is_refused_where_a_gateway_has_the_accounts_name() {
    let gateway = |id: &str| {
        configured(&format!(
            r#"{{"provider": {{"{id}": {{"options": {{"baseURL": "https://gw.example.invalid/v1"}}}}}}}}"#
        ))
    };
    assert_eq!(
        refusal(
            Account::ChatGpt,
            &before(true, gateway("chatgpt"), false),
            true
        ),
        Some(t!(desktop_accounts_sign_in_chatgpt_is_a_gateway))
    );
    assert_eq!(
        refusal(Account::Xai, &before(true, gateway("xai"), false), true),
        Some(t!(desktop_accounts_sign_in_xai_is_a_gateway))
    );
    assert_eq!(
        refusal(Account::Xai, &before(true, gateway("chatgpt"), false), true),
        None
    );
}

#[test]
fn every_way_a_sign_in_stops_has_a_sentence_of_its_own_words() {
    use bravebot_skus::chatgpt::SignInError as C;
    use bravebot_skus::oauth::AccountError;
    use bravebot_skus::xai::SignInError as X;
    let detail = "/home/a/.bravebot/placeholder-detail";
    let classes = || {
        [
            Class::Refused,
            Class::RateLimited,
            Class::Unavailable,
            Class::Incomplete,
            Class::Transport,
        ]
    };
    let mut chatgpt = vec![
        C::Listener,
        C::TimedOut,
        C::NoCode,
        C::NoClient,
        C::ClientChanged,
        C::NoUseScope,
        C::IdToken,
        C::Clock,
        C::Cancelled,
        C::Store(AccountError::Unwritable {
            detail: detail.into(),
        }),
    ];
    chatgpt.extend(classes().map(C::Exchange));
    chatgpt.extend(classes().map(C::KeySet));
    for stopped in &chatgpt {
        let said = said_of_chatgpt(stopped);
        assert!(
            !said.is_empty() && !said.contains(detail),
            "{stopped}: {said}"
        );
    }
    let mut xai = vec![
        X::AccessDenied,
        X::AuthorizationDenied,
        X::Expired,
        X::TimedOut,
        X::Clock,
        X::Cancelled,
        X::Store(AccountError::Nowhere {
            detail: detail.into(),
        }),
    ];
    xai.extend(classes().map(X::Code));
    xai.extend(classes().map(X::Exchange));
    for stopped in &xai {
        let said = said_of_xai(stopped);
        assert!(
            !said.is_empty() && !said.contains(detail),
            "{stopped}: {said}"
        );
    }
}
