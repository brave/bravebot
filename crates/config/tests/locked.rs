//! `lock` cannot be undone, so the process-wide switch is tested in a binary of its own.

use bravebot_config::Narrowing;

#[test]
fn a_locked_process_makes_the_bypass_mode_unreachable_whatever_the_layers_said() {
    assert!(!bravebot_config::locked());
    assert!(!Narrowing::default().makes_bypass_unreachable());

    bravebot_config::lock();

    assert!(bravebot_config::locked());
    let nothing_said = Narrowing::default();
    assert!(nothing_said.makes_bypass_unreachable());
    assert!(
        nothing_said
            .strictest(Narrowing::default())
            .makes_bypass_unreachable()
    );
}
