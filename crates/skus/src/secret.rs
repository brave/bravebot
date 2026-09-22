//! A buffer this crate owns a credential in, cleared when it goes.
//!
//! [CRED-23](../../../docs/specs/credential-protection.md#CRED-23) promises that a buffer this
//! program owns a credential in is cleared when it is dropped rather than returned to the
//! allocator intact. A single-use subscription credential is one of those: it is a bearer value
//! for the length of a session, and the batch holding it is hundreds of them.
//!
//! Spelled here rather than taken from the crate above, for the same reason and with the same cost
//! as the profile variables and the file mode in [`crate::store`]:
//! [LAYER-1](../../../docs/specs/layering.md#LAYER-1) gives this crate no dependency on another
//! crate here, so reaching the configuration crate's version of this would be a layering change
//! rather than a line. What has to hold across the two copies is that the bytes are overwritten
//! where they lie and that reading them is visible at the call site.

use std::fmt;

/// A value that must not be printed.
///
/// `Debug` and `Display` are deliberately redacting, and the inner value is only reachable through
/// [`Secret::expose`], a name chosen so that reading a credential is visible at the call site
/// during review.
///
/// Intentionally missing: equality. An operator that answers a question about the bytes recovers
/// them a guess at a time, and the derived answer takes time proportional to the shared prefix.
/// Nothing here compares one credential against another, so what would be needed is a constant
/// time comparison nobody calls.
///
/// Constructing one and asking whether it is empty is ordinary, which is what makes the refusal
/// below a refusal of equality rather than of the name:
///
/// ```
/// use bravebot_skus::Secret;
/// assert!(!Secret::new("a").is_empty());
/// ```
///
/// Comparing two of them does not compile:
///
/// ```compile_fail
/// use bravebot_skus::Secret;
/// let _ = Secret::new("a") == Secret::new("a");
/// ```
///
/// Dropping one overwrites its buffer, so a credential is not handed back to the allocator intact.
/// Cloning makes a second buffer that is cleared the same way when it goes.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Read the secret. Call sites should be rare and obvious.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Clear the buffer rather than return it to the allocator holding a credential.
///
/// What this reaches is the buffer this type owns, which is what CRED-23 promises and all it
/// promises. A value that was copied on its way in, by an allocator growing a `String` or by the
/// TLS library between here and a socket, left a copy nothing here holds a pointer to.
impl Drop for Secret {
    fn drop(&mut self) {
        scrub(&mut self.0);
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Overwrite a string's bytes where they lie, leaving the buffer as many zero bytes long as the
/// value was.
///
/// `clear` is not this and is the mistake it exists to avoid: it sets the length to nothing and
/// leaves every byte where it was, so the credential is still in the allocation when the allocator
/// hands it to whoever asks next. Clearing and then pushing the replacement back writes over the
/// original bytes in place, because `clear` keeps the capacity and the replacement is exactly as
/// long as what it replaces, so nothing reallocates.
///
/// The write is held down by handing the bytes to [`std::hint::black_box`]. Nothing reads them
/// back, and a compiler that can see the whole life of the buffer is entitled to delete a store no
/// one observes; an opaque use of the bytes is how safe code says otherwise. It is a barrier rather
/// than a guarantee the language makes, which is the price of doing this in a crate that forbids
/// `unsafe` and so cannot write the bytes volatile.
pub(crate) fn scrub(value: &mut String) {
    let length = value.len();
    value.clear();
    value.extend(std::iter::repeat_n('\0', length));
    std::hint::black_box(value.as_bytes());
}

/// A parsed JSON document that clears every string in itself when it goes.
///
/// The credential file is read as JSON, so a token is in the parsed tree before it is in a
/// [`Secret`], and that copy is as much this program's buffer as the field it ends up in. A guard
/// rather than a call at each place a document dies: `decode` has six early returns, and the one
/// that forgets the call is the one a malformed file takes.
///
/// Every string is cleared rather than the ones under a name that looks like a credential. A
/// document is small, the walk is cheap, and a rule about which names hold a secret is a rule to
/// get wrong when the format changes.
pub(crate) struct Document(serde_json::Value);

impl Document {
    pub(crate) fn of(value: serde_json::Value) -> Self {
        Self(value)
    }

    pub(crate) fn read(&self) -> &serde_json::Value {
        &self.0
    }
}

impl Drop for Document {
    fn drop(&mut self) {
        scrub_document(&mut self.0);
    }
}

/// Overwrite every string in a parsed document, to the bottom of the blocks it was written in.
///
/// Object keys are left alone: a key is a field name this crate wrote or expected, and the
/// serialiser owns that buffer rather than this.
pub(crate) fn scrub_document(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(text) => scrub(text),
        serde_json::Value::Array(items) => items.iter_mut().for_each(scrub_document),
        serde_json::Value::Object(fields) => {
            fields
                .iter_mut()
                .for_each(|(_, field)| scrub_document(field));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A credential that reaches a log or a panic message is a credential in a file somebody
    /// forwards, so the redaction is the point of the type rather than a nicety.
    #[test]
    fn a_secret_prints_as_redacted_rather_than_as_its_value() {
        let secret = Secret::new("AQIDBAUGBwgJ-unblinded-token");

        assert_eq!(format!("{secret:?}"), "Secret(<redacted>)");
        assert_eq!(format!("{secret}"), "<redacted>");
        assert!(!format!("{secret:?}").contains("unblinded"));
    }

    /// The credential has to be gone from the allocation, not just from the length.
    ///
    /// Both halves matter and neither alone says it. A buffer of zeros at a fresh address leaves
    /// the original bytes where the allocator can hand them on, and an unmoved buffer still holding
    /// the value is what `String::clear` produces: the length reads zero and every byte is still
    /// there.
    #[test]
    fn scrubbing_overwrites_the_bytes_where_they_lie() {
        let mut value = String::from("AQIDBAUGBwgJ-unblinded-token");
        let length = value.len();
        let address = value.as_ptr();

        scrub(&mut value);

        assert_eq!(
            value.as_ptr(),
            address,
            "the buffer moved, so the credential is still in the one that was left behind"
        );
        assert_eq!(
            value.as_bytes(),
            vec![0u8; length],
            "the buffer the credential was in still holds bytes of it"
        );
    }

    /// Bytes rather than characters, because a value that is not ASCII has more of the first than
    /// the second and the tail of it is what a count of characters would leave behind.
    ///
    /// A token is base64 and so is ASCII, but the walk below reaches every string in a document the
    /// server sent, and nothing bounds what is in one of those.
    #[test]
    fn scrubbing_counts_the_bytes_rather_than_the_characters() {
        let mut value = String::from("token-\u{4e16}\u{754c}");
        let length = value.len();
        assert!(length > value.chars().count(), "the fixture is not ASCII");

        scrub(&mut value);

        assert_eq!(
            value.len(),
            length,
            "the buffer is as long as the value was"
        );
        assert!(
            value.bytes().all(|byte| byte == 0),
            "a byte of the value survived the scrub"
        );
    }

    /// The credential file nests its tokens two deep, so a walk that stops at the top level clears
    /// the version number and leaves every token in the document it just read.
    #[test]
    fn scrubbing_a_document_reaches_a_token_inside_the_blocks_it_was_written_in() {
        let mut document = serde_json::json!({
            "version": 1,
            "order_id": "8f1c4a2e",
            "credentials": [{ "unblinded": "AQIDBAUGBwgJ-unblinded-token", "spent": false }],
        });

        scrub_document(&mut document);

        let token = document["credentials"][0]["unblinded"]
            .as_str()
            .expect("the shape of the document is unchanged");
        assert!(
            token.bytes().all(|byte| byte == 0),
            "the token is still in the parsed document: {token:?}"
        );
        assert_eq!(
            document["order_id"].as_str().map(str::len),
            Some(8),
            "the buffer is as long as the value was"
        );
    }
}
