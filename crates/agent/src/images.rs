//! Pinning the image an OpenShell server runs in to the digest its registry gives it now.
//!
//! A tag such as `python:3.13-slim` names whatever its registry points it at, so a declaration
//! naming one would start a different image the day the tag moves, under an approval given for the
//! old one. Pinned, the reference carries `@sha256:` and the digest of the manifest it named when
//! it was declared, which is in the declaration's own digest and so in what the person approves.
//!
//! The digest is the sha256 of the manifest's bytes, worked out here rather than taken from a
//! header the registry sends: that is what a digest is, and what the engine pulling the image
//! checks. The manifest is asked for as an index first, so the digest names every platform's
//! image and the sandbox gets the one for the machine it runs on.
//!
//! Only an anonymous pull is made. A private image, a registry that cannot be reached, or one that
//! refuses is not pinned, and the person is told to give the reference with its digest.

use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::RecordingSink;
use bravebot_core::label::Label;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_net::{Egress, Request};
use sha2::{Digest as _, Sha256};

/// The manifest types asked for, an index first, so a multi-platform image is pinned whole.
const ACCEPT: &str = "application/vnd.oci.image.index.v1+json, \
    application/vnd.docker.distribution.manifest.list.v2+json, \
    application/vnd.oci.image.manifest.v1+json, \
    application/vnd.docker.distribution.manifest.v2+json";

/// The registry an image reference names no registry of.
const DOCKER_HUB: &str = "docker.io";

/// An image reference, split as a registry reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    /// The registry, as written, or Docker Hub where none was.
    pub registry: String,
    /// The repository within it, with Docker Hub's `library/` where a bare name implies it.
    pub repository: String,
    /// The tag, `latest` where none was written.
    pub tag: String,
    /// The digest, where the reference is already pinned.
    pub digest: Option<String>,
}

impl Reference {
    /// `text` read as an image reference, where it is one.
    pub fn parse(text: &str) -> Option<Self> {
        let (named, digest) = match text.split_once('@') {
            Some((named, digest)) => (named, Some(digest)),
            None => (text, None),
        };
        if digest.is_some_and(|digest| !is_digest(digest)) {
            return None;
        }
        // A colon after the last slash ends the name with a tag; one before it is a port.
        let (name, tag) = match named.rsplit_once(':') {
            Some((name, tag)) if !tag.contains('/') => (name, Some(tag)),
            _ => (named, None),
        };
        if tag.is_some_and(|tag| !is_tag(tag)) {
            return None;
        }
        // A first part that looks like a host is the registry, and must be one: `./srv` and
        // `../x` are paths, not a registry and a repository, and are refused.
        let (registry, repository) = match name.split_once('/') {
            Some((first, rest))
                if first.contains('.') || first.contains(':') || first == "localhost" =>
            {
                if !is_registry(first) {
                    return None;
                }
                (first.to_string(), rest.to_string())
            }
            _ => (DOCKER_HUB.to_string(), name.to_string()),
        };
        let repository = match registry == DOCKER_HUB && !repository.contains('/') {
            true => format!("library/{repository}"),
            false => repository,
        };
        if !is_repository(&repository) || registry.is_empty() {
            return None;
        }
        Some(Self {
            registry,
            repository,
            tag: tag.unwrap_or("latest").to_string(),
            digest: digest.map(str::to_string),
        })
    }

    /// The host the registry's API is served from.
    fn host(&self) -> &str {
        match self.registry.as_str() {
            DOCKER_HUB => "registry-1.docker.io",
            other => other,
        }
    }

    /// Where the manifest the tag names is read.
    pub fn manifest_url(&self) -> String {
        format!(
            "https://{}/v2/{}/manifests/{}",
            self.host(),
            self.repository,
            self.tag
        )
    }

    /// Where an anonymous token to pull the repository is asked for, at the registries that need
    /// one even for a public image. Elsewhere the manifest is asked for with none.
    pub fn token_url(&self) -> Option<String> {
        match self.registry.as_str() {
            DOCKER_HUB => Some(format!(
                "https://auth.docker.io/token?service=registry.docker.io&scope=repository:{}:pull",
                self.repository
            )),
            "ghcr.io" => Some(format!(
                "https://ghcr.io/token?scope=repository:{}:pull",
                self.repository
            )),
            _ => None,
        }
    }
}

/// Why an image was not pinned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unpinned {
    /// The text is not an image reference.
    NotAReference,
    /// The registry answered with this status instead of a manifest.
    Refused(u16),
    /// The registry could not be reached, for this reason.
    Unreachable(String),
}

/// `image` with the digest of the manifest it names now, as `image@sha256:…`, or `image` as it is
/// where it already carries one.
///
/// Asks the registry, through the one egress gate every request passes.
pub fn pinned(image: &str) -> Result<String, Unpinned> {
    let reference = Reference::parse(image).ok_or(Unpinned::NotAReference)?;
    if reference.digest.is_some() {
        return Ok(image.to_string());
    }
    let mut sink = RecordingSink::new();
    let mut routing = Routing::new();
    routing.insert_trusted("image", image);
    let mut policy = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .map_err(|denial| Unpinned::Unreachable(denial.to_string()))?;
    let egress = Egress::new();
    let label = Label::untrusted_public();

    let token = match reference.token_url() {
        Some(url) => {
            let answer = egress
                .fetch(&mut policy, Request::get(url), label)
                .map_err(|error| Unpinned::Unreachable(error.to_string()))?;
            if answer.status != 200 {
                return Err(Unpinned::Refused(answer.status));
            }
            // The token is the registry's own handshake: it goes back to the registry that issued
            // it, scoped to pulling this one repository, and is never shown or kept.
            let decoding = policy.decode_transport("a registry's anonymous pull token", label);
            let (body, _) = decoding.decode(answer.body);
            token_in(&body)
        }
        None => None,
    };

    let mut request = Request::get(reference.manifest_url()).header("Accept", ACCEPT);
    if let Some(token) = token {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    let answer = egress
        .fetch(&mut policy, request, label)
        .map_err(|error| Unpinned::Unreachable(error.to_string()))?;
    if answer.status != 200 || answer.truncated {
        return Err(Unpinned::Refused(answer.status));
    }
    // Only the digest is released, worked out without the manifest being read.
    let digest = policy.render_in_place("image_manifest", &answer.body, |body| digest_of(&body));
    let proof = policy.authorise_display_release("the digest of an image's manifest");
    let digest = digest.declassify(&proof);
    let _ = policy.finish();
    Ok(format!("{image}@{digest}"))
}

/// The digest of `manifest`, as a registry writes one.
fn digest_of(manifest: &[u8]) -> String {
    let hex: String = Sha256::digest(manifest)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("sha256:{hex}")
}

/// The token in a registry's token answer, under either name registries give it.
fn token_in(body: &[u8]) -> Option<String> {
    let answer: serde_json::Value = serde_json::from_slice(body).ok()?;
    ["token", "access_token"]
        .iter()
        .find_map(|key| answer.get(key)?.as_str())
        .filter(|token| !token.is_empty() && token.bytes().all(|b| b.is_ascii_graphic()))
        .map(str::to_string)
}

fn is_digest(text: &str) -> bool {
    text.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    })
}

fn is_tag(text: &str) -> bool {
    (1..=128).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
        && !text.starts_with(['.', '-'])
}

/// Whether `text` is a repository path: parts of lower-case letters and digits, each starting and
/// ending with one, joined inside by `.`, `_` or `-`, as the OCI distribution grammar has them.
fn is_repository(text: &str) -> bool {
    let edge = |b: Option<u8>| b.is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
    !text.is_empty()
        && text.split('/').all(|part| {
            edge(part.bytes().next())
                && edge(part.bytes().last())
                && part.bytes().all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
                })
        })
}

/// Whether `text` is a registry: a host name of letters, digits and inner `-` in dot-separated
/// labels, and optionally `:` and a port.
fn is_registry(text: &str) -> bool {
    let (host, port) = match text.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (text, None),
    };
    let label = |label: &str| {
        !label.is_empty()
            && label.len() <= 63
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    };
    host.split('.').all(label)
        && port.is_none_or(|port| {
            (1..=5).contains(&port.len()) && port.bytes().all(|b| b.is_ascii_digit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest() -> String {
        format!("sha256:{}", "a".repeat(64))
    }

    #[test]
    fn a_reference_is_read_as_a_registry_reads_it() {
        let read = |text: &str| Reference::parse(text).expect(text);
        let hub = read("python:3.13-slim");
        assert_eq!(
            (
                hub.registry.as_str(),
                hub.repository.as_str(),
                hub.tag.as_str()
            ),
            ("docker.io", "library/python", "3.13-slim")
        );
        assert_eq!(
            hub.manifest_url(),
            "https://registry-1.docker.io/v2/library/python/manifests/3.13-slim"
        );
        assert!(
            hub.token_url()
                .is_some_and(|url| url.contains("repository:library/python:pull"))
        );

        let user = read("bitnami/redis");
        assert_eq!(
            (user.repository.as_str(), user.tag.as_str()),
            ("bitnami/redis", "latest")
        );

        let ghcr = read("ghcr.io/owner/server:1.2");
        assert_eq!(
            ghcr.manifest_url(),
            "https://ghcr.io/v2/owner/server/manifests/1.2"
        );
        assert!(ghcr.token_url().is_some());

        let port = read("localhost:5000/team/srv");
        assert_eq!(
            (
                port.registry.as_str(),
                port.repository.as_str(),
                port.tag.as_str()
            ),
            ("localhost:5000", "team/srv", "latest")
        );
        assert_eq!(port.token_url(), None);

        let pinned = read(&format!("python:3.13-slim@{}", digest()));
        assert_eq!(pinned.digest, Some(digest()));
    }

    #[test]
    fn what_is_not_a_reference_is_refused() {
        for text in [
            "",
            "Python",
            "python:",
            "python:-bad",
            "python@sha256:short",
            "python@md5:aaaa",
            "a//b",
            "python:3.13 slim",
            "./srv@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "../x@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "registry..example/x",
            "host:port/x",
            "host:123456/x",
            "-host.example/x",
            ".hidden/x",
            "owner/.x",
            "owner/x-",
        ] {
            assert_eq!(Reference::parse(text), None, "{text:?}");
        }
    }

    /// A reference that already carries a digest is kept as written, with nothing asked of a
    /// registry; one that is not a reference is refused the same way.
    #[test]
    fn a_pinned_reference_is_kept_and_nothing_is_fetched() {
        let image = format!("python:3.13-slim@{}", digest());
        assert_eq!(pinned(&image), Ok(image.clone()));
        assert_eq!(pinned("Not An Image"), Err(Unpinned::NotAReference));
    }

    #[test]
    fn the_digest_is_the_sha256_of_the_manifest_and_a_token_is_read_under_either_name() {
        assert_eq!(
            digest_of(b""),
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(token_in(br#"{"token":"abc"}"#), Some("abc".into()));
        assert_eq!(token_in(br#"{"access_token":"xyz"}"#), Some("xyz".into()));
        assert_eq!(token_in(br#"{"token":"a b"}"#), None);
        assert_eq!(token_in(b"not json"), None);
    }

    /// Against Docker Hub, where `BRAVEBOT_OPENSHELL_TESTS=1` says the network may be used.
    #[test]
    fn a_tag_on_docker_hub_is_pinned_to_its_index() {
        if std::env::var("BRAVEBOT_OPENSHELL_TESTS").as_deref() != Ok("1") {
            return;
        }
        let pinned = pinned("python:3.13-slim").expect("pinned");
        let reference = Reference::parse(&pinned).expect("a reference");
        assert!(reference.digest.is_some(), "{pinned}");
        assert!(pinned.starts_with("python:3.13-slim@sha256:"), "{pinned}");
    }
}
