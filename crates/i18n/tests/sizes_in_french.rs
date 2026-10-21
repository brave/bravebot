//! The size notes in a French catalog, in a process of their own because the locale is chosen once
//! per process (LOCALE-7).

use bravebot_i18n::sizes::{attachment_too_large, paste_too_large};

/// French writes `Mo` and a decimal comma, which is why the desktop asks for these notes rather
/// than writing its own.
#[test]
fn the_size_notes_are_said_in_the_chosen_catalog() {
    bravebot_i18n::init(bravebot_i18n::resolve("fr"));
    assert_eq!(
        paste_too_large(20 * 1024 * 1024, 10 * 1024 * 1024),
        "cette image fait 20,0 Mo, et un collage en porte au plus 10,0 Mo"
    );
    assert_eq!(
        attachment_too_large("scan.pdf", 9 * 1024 * 1024, 8 * 1024 * 1024),
        "scan.pdf fait 9,0 Mo, et une pièce jointe en porte au plus 8,0 Mo"
    );
}
