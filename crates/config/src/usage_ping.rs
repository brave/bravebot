//! Whether the daily usage ping is on, and if not, what turned it off.
//!
//! The rule alone, over inputs its caller gathered, so the terminal, the desktop bridge and the
//! crate that sends the ping cannot each resolve the switches in a different order. Gathering the
//! inputs reads the state directory, which this crate does not own; `bravebot-session` does that.

use crate::env_var;

/// Whether the ping goes, and the reason when it does not.
///
/// Each reason names the one switch a person would have to change, so a switch shown as locked can
/// say which file or variable holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsagePing {
    /// Nothing turned it off.
    On,
    /// The person turned it off with the switch in `/config` or the desktop settings.
    OffByUser,
    /// `usagePing` is `false` in the person's own settings file.
    OffBySetting,
    /// A variable in the environment turned it off. Holds the variable's name.
    OffByEnvironment(&'static str),
    /// `usagePing` is `false` in the managed file.
    OffByAdmin,
    /// The session records nothing, so it has no record of when it last pinged.
    Incognito,
}

impl UsagePing {
    /// Whether a ping may be sent.
    pub fn is_on(self) -> bool {
        self == Self::On
    }

    /// Whether the switch a person sees may change the answer.
    ///
    /// Only the switch's own two answers can be changed by it. Every other reason outranks the
    /// switch, so a switch shown as changeable there would do nothing when used.
    pub fn person_may_change(self) -> bool {
        matches!(self, Self::On | Self::OffByUser)
    }
}

/// What each switch said, gathered by the caller.
#[derive(Clone, Copy, Debug, Default)]
pub struct Inputs<'a> {
    /// The session records nothing.
    pub incognito: bool,
    /// The managed file wrote `usagePing: false`.
    pub managed_off: bool,
    /// The value of [`env_var::USAGE_PING`], where it is set.
    pub usage_ping_variable: Option<&'a str>,
    /// The value of [`env_var::DO_NOT_TRACK`], where it is set.
    pub do_not_track: Option<&'a str>,
    /// What the person's own settings file says about `usagePing`.
    pub setting: Option<bool>,
    /// What the person last chose with the switch, where they chose anything.
    pub switch: Option<bool>,
}

/// Resolve the switches, strongest first.
///
/// Every input can only turn the ping off. An `on` from the switch or the settings file is no
/// stronger than saying nothing, so no layer can turn back on what a stronger one turned off, and
/// a person who turned it off with the switch is not overruled by a settings file saying `true`.
pub fn resolve(inputs: Inputs<'_>) -> UsagePing {
    if inputs.incognito {
        return UsagePing::Incognito;
    }
    if inputs.managed_off {
        return UsagePing::OffByAdmin;
    }
    if let Some(variable) = environment_off(inputs.usage_ping_variable, inputs.do_not_track) {
        return UsagePing::OffByEnvironment(variable);
    }
    if inputs.setting == Some(false) {
        return UsagePing::OffBySetting;
    }
    if inputs.switch == Some(false) {
        return UsagePing::OffByUser;
    }
    UsagePing::On
}

/// The variable that turned the ping off, where one did.
///
/// [`env_var::USAGE_PING`] is named first when both are set, since it is the one this program
/// documents.
fn environment_off(usage_ping: Option<&str>, do_not_track: Option<&str>) -> Option<&'static str> {
    if usage_ping.map(str::trim) == Some("0") {
        return Some(env_var::USAGE_PING);
    }
    let asked = do_not_track.map(str::trim).is_some_and(|value| {
        !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false")
    });
    asked.then_some(env_var::DO_NOT_TRACK)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With nothing said, the ping is on.
    #[test]
    fn the_ping_is_on_when_nothing_turned_it_off() {
        assert_eq!(resolve(Inputs::default()), UsagePing::On);
        let all_on = Inputs {
            setting: Some(true),
            switch: Some(true),
            usage_ping_variable: Some("1"),
            do_not_track: Some("0"),
            ..Inputs::default()
        };
        assert_eq!(resolve(all_on), UsagePing::On);
    }

    /// Each switch alone turns the ping off and is named as the reason.
    #[test]
    fn each_switch_alone_turns_the_ping_off_and_is_named() {
        let cases = [
            (
                Inputs {
                    incognito: true,
                    ..Inputs::default()
                },
                UsagePing::Incognito,
            ),
            (
                Inputs {
                    managed_off: true,
                    ..Inputs::default()
                },
                UsagePing::OffByAdmin,
            ),
            (
                Inputs {
                    usage_ping_variable: Some("0"),
                    ..Inputs::default()
                },
                UsagePing::OffByEnvironment(env_var::USAGE_PING),
            ),
            (
                Inputs {
                    do_not_track: Some("1"),
                    ..Inputs::default()
                },
                UsagePing::OffByEnvironment(env_var::DO_NOT_TRACK),
            ),
            (
                Inputs {
                    setting: Some(false),
                    ..Inputs::default()
                },
                UsagePing::OffBySetting,
            ),
            (
                Inputs {
                    switch: Some(false),
                    ..Inputs::default()
                },
                UsagePing::OffByUser,
            ),
        ];
        for (inputs, wanted) in cases {
            assert_eq!(resolve(inputs), wanted, "{inputs:?}");
            assert!(!wanted.is_on());
        }
    }

    /// The switch saying on cannot turn back on what a stronger layer turned off, and the reason
    /// given is the stronger layer, so the switch is shown as locked.
    #[test]
    fn the_switch_cannot_turn_the_ping_back_on_over_a_stronger_layer() {
        let switched_on = Inputs {
            switch: Some(true),
            ..Inputs::default()
        };
        for (inputs, wanted) in [
            (
                Inputs {
                    setting: Some(false),
                    ..switched_on
                },
                UsagePing::OffBySetting,
            ),
            (
                Inputs {
                    usage_ping_variable: Some("0"),
                    ..switched_on
                },
                UsagePing::OffByEnvironment(env_var::USAGE_PING),
            ),
            (
                Inputs {
                    managed_off: true,
                    ..switched_on
                },
                UsagePing::OffByAdmin,
            ),
            (
                Inputs {
                    incognito: true,
                    ..switched_on
                },
                UsagePing::Incognito,
            ),
        ] {
            let resolved = resolve(inputs);
            assert_eq!(resolved, wanted, "{inputs:?}");
            assert!(!resolved.person_may_change(), "{inputs:?}");
        }
    }

    /// A settings file saying `true` does not overrule a person who turned the ping off with the
    /// switch.
    #[test]
    fn a_setting_of_true_does_not_overrule_the_switch_turned_off() {
        let inputs = Inputs {
            setting: Some(true),
            switch: Some(false),
            ..Inputs::default()
        };
        assert_eq!(resolve(inputs), UsagePing::OffByUser);
        assert!(UsagePing::OffByUser.person_may_change());
        assert!(UsagePing::On.person_may_change());
    }

    /// The strongest reason is the one named when several layers turned the ping off.
    #[test]
    fn the_strongest_layer_is_named_when_several_turned_it_off() {
        let everything = Inputs {
            incognito: true,
            managed_off: true,
            usage_ping_variable: Some("0"),
            do_not_track: Some("1"),
            setting: Some(false),
            switch: Some(false),
        };
        assert_eq!(resolve(everything), UsagePing::Incognito);
        let no_incognito = Inputs {
            incognito: false,
            ..everything
        };
        assert_eq!(resolve(no_incognito), UsagePing::OffByAdmin);
        let no_admin = Inputs {
            managed_off: false,
            ..no_incognito
        };
        assert_eq!(
            resolve(no_admin),
            UsagePing::OffByEnvironment(env_var::USAGE_PING)
        );
        let no_variable = Inputs {
            usage_ping_variable: None,
            ..no_admin
        };
        assert_eq!(
            resolve(no_variable),
            UsagePing::OffByEnvironment(env_var::DO_NOT_TRACK)
        );
        let no_environment = Inputs {
            do_not_track: None,
            ..no_variable
        };
        assert_eq!(resolve(no_environment), UsagePing::OffBySetting);
    }

    /// Only `0` stops the ping through its own variable, and `DO_NOT_TRACK` stops it for any value
    /// other than an empty one, `0` or `false`.
    #[test]
    fn which_variable_values_turn_the_ping_off() {
        for (value, off) in [
            ("0", true),
            (" 0 ", true),
            ("1", false),
            ("false", false),
            ("", false),
        ] {
            assert_eq!(
                environment_off(Some(value), None).is_some(),
                off,
                "BRAVEBOT_USAGE_PING={value:?}"
            );
        }
        for (value, off) in [
            ("1", true),
            ("true", true),
            ("yes", true),
            ("0", false),
            ("false", false),
            ("FALSE", false),
            ("", false),
            ("  ", false),
        ] {
            assert_eq!(
                environment_off(None, Some(value)).is_some(),
                off,
                "DO_NOT_TRACK={value:?}"
            );
        }
        assert_eq!(environment_off(None, None), None);
    }
}
