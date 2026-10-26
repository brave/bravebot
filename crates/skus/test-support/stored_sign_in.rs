//! The text of the file `auth login chatgpt` stores, for a test that needs a sign-in stored in the
//! home a program runs in. Written by hand, so that what the program reads is the format and not
//! what the program that writes it would produce.

/// The file for an access token that expires `expires_at` seconds after 1970-01-01.
pub fn stored_sign_in(
    access_token: &str,
    refresh_token: &str,
    expires_at: u64,
    client_id: &str,
    host_id: &str,
) -> String {
    serde_json::json!({
        "version": 1,
        "access_token": access_token,
        "refresh_token": refresh_token,
        "expires_at": expires_at,
        "client_id": client_id,
        "host_id": host_id,
    })
    .to_string()
}
