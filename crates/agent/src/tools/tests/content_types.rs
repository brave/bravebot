use super::*;

/// A server's `Content-Type` header is shown on the person's screen, so what it carries must
/// not reach the terminal as escape sequences or control characters, and a long one is cut.
#[test]
fn a_content_type_is_shown_without_anything_a_terminal_would_obey() {
    assert_eq!(shown_content_type(Some("image/png")), "image/png");
    assert_eq!(shown_content_type(None), "no content type");
    assert_eq!(
        shown_content_type(Some("text/plain\u{1b}[2J\u{7}\r\nx")),
        "text/plain[2Jx"
    );
    assert_eq!(shown_content_type(Some(&"a".repeat(500))).len(), 100);
}
