//! Whether crash reports are on, and if not, what turned them off (CRASH-4).
//!
//! The rule alone, over inputs its caller gathered, so the desktop bridge and the launch that
//! installs the hook cannot each resolve the switches in a different order. Gathering the inputs
//! reads the state directory, which this crate does not own; `bravebot-backtrace` does that.

use crate::env_var;

/// Whether reports are on, and the reason when they are not.
///
/// Each reason names the one switch a person would have to change, so a switch shown as locked can
/// say which file or variable holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrashReports {
    /// Nothing turned them off.
    On,
    /// `crashReports` is `false` in the person's own settings file, which the switch in the desktop
    /// settings writes.
    OffByPerson,
    /// A variable in the environment turned them off. Holds the variable's name.
    OffByEnvironment(&'static str),
    /// `crashReports` is `false` in the managed file.
    OffByAdmin,
    /// The session has no state directory to leave a report in.
    Incognito,
}

impl CrashReports {
    /// Whether reports may be written and sent.
    pub fn is_on(self) -> bool {
        self == Self::On
    }

    /// Whether the switch a person sees may change the answer.
    ///
    /// Only the switch's own two answers can be changed by it. Every other reason outranks the
    /// switch, so a switch shown as changeable there would do nothing when used.
    pub fn person_may_change(self) -> bool {
        matches!(self, Self::On | Self::OffByPerson)
    }
}

/// What each switch said, gathered by the caller.
#[derive(Clone, Copy, Debug, Default)]
pub struct Inputs<'a> {
    /// The session has no state directory, which is what an incognito session is given.
    pub incognito: bool,
    /// The managed file wrote `crashReports: false`.
    pub managed_off: bool,
    /// The value of [`env_var::CRASH_REPORTS`], where it is set.
    pub variable: Option<&'a str>,
    /// What the person's own settings file says about `crashReports`.
    pub setting: Option<bool>,
}

/// Resolve the switches, strongest first.
///
/// Every input can only turn reports off. A `true` from the settings file is no stronger than
/// saying nothing, so no layer turns back on what a stronger one turned off. Only the exact value
/// `0` counts for the variable and only a boolean `false` for the file, so a misspelling leaves
/// reports on.
pub fn resolve(inputs: Inputs<'_>) -> CrashReports {
    if inputs.incognito {
        return CrashReports::Incognito;
    }
    if inputs.managed_off {
        return CrashReports::OffByAdmin;
    }
    if inputs.variable == Some("0") {
        return CrashReports::OffByEnvironment(env_var::CRASH_REPORTS);
    }
    if inputs.setting == Some(false) {
        return CrashReports::OffByPerson;
    }
    CrashReports::On
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With nothing said, and with every layer saying on, reports are on.
    #[test]
    fn reports_are_on_when_nothing_turned_them_off() {
        assert_eq!(resolve(Inputs::default()), CrashReports::On);
        let all_on = Inputs {
            setting: Some(true),
            variable: Some("1"),
            ..Inputs::default()
        };
        assert_eq!(resolve(all_on), CrashReports::On);
    }

    /// Each switch alone turns reports off and is named as the reason. The faults this rejects
    /// are a switch that does nothing and one that is named as another.
    #[test]
    fn each_switch_alone_turns_reports_off_and_is_named() {
        let cases = [
            (
                Inputs {
                    incognito: true,
                    ..Inputs::default()
                },
                CrashReports::Incognito,
            ),
            (
                Inputs {
                    managed_off: true,
                    ..Inputs::default()
                },
                CrashReports::OffByAdmin,
            ),
            (
                Inputs {
                    variable: Some("0"),
                    ..Inputs::default()
                },
                CrashReports::OffByEnvironment(env_var::CRASH_REPORTS),
            ),
            (
                Inputs {
                    setting: Some(false),
                    ..Inputs::default()
                },
                CrashReports::OffByPerson,
            ),
        ];
        for (inputs, wanted) in cases {
            assert_eq!(resolve(inputs), wanted, "{inputs:?}");
            assert!(!wanted.is_on());
        }
    }

    /// The person's own `true` cannot turn reports back on over a stronger layer, and the reason
    /// given is the stronger layer, so the switch is shown as locked. The fault this rejects is a
    /// `true` in the settings file outranking the variable, the managed file or incognito.
    #[test]
    fn a_setting_of_true_cannot_turn_reports_back_on_over_a_stronger_layer() {
        let switched_on = Inputs {
            setting: Some(true),
            ..Inputs::default()
        };
        for (inputs, wanted) in [
            (
                Inputs {
                    variable: Some("0"),
                    ..switched_on
                },
                CrashReports::OffByEnvironment(env_var::CRASH_REPORTS),
            ),
            (
                Inputs {
                    managed_off: true,
                    ..switched_on
                },
                CrashReports::OffByAdmin,
            ),
            (
                Inputs {
                    incognito: true,
                    ..switched_on
                },
                CrashReports::Incognito,
            ),
        ] {
            let resolved = resolve(inputs);
            assert_eq!(resolved, wanted, "{inputs:?}");
            assert!(!resolved.person_may_change(), "{inputs:?}");
        }
        assert!(CrashReports::On.person_may_change());
        assert!(CrashReports::OffByPerson.person_may_change());
    }

    /// The strongest reason is the one named when several layers turned reports off.
    #[test]
    fn the_strongest_layer_is_named_when_several_turned_reports_off() {
        let everything = Inputs {
            incognito: true,
            managed_off: true,
            variable: Some("0"),
            setting: Some(false),
        };
        assert_eq!(resolve(everything), CrashReports::Incognito);
        let no_incognito = Inputs {
            incognito: false,
            ..everything
        };
        assert_eq!(resolve(no_incognito), CrashReports::OffByAdmin);
        let no_admin = Inputs {
            managed_off: false,
            ..no_incognito
        };
        assert_eq!(
            resolve(no_admin),
            CrashReports::OffByEnvironment(env_var::CRASH_REPORTS)
        );
        let no_variable = Inputs {
            variable: None,
            ..no_admin
        };
        assert_eq!(resolve(no_variable), CrashReports::OffByPerson);
    }

    /// Only the exact value `0` turns reports off through the variable, so a misspelling leaves
    /// them on (CRASH-4).
    #[test]
    fn only_the_exact_value_zero_turns_reports_off_through_the_variable() {
        for loose in ["", "1", "false", "off", "no", " 0", "0 ", "00"] {
            let inputs = Inputs {
                variable: Some(loose),
                ..Inputs::default()
            };
            assert_eq!(resolve(inputs), CrashReports::On, "{loose:?}");
        }
    }
}
