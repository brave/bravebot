//! Taking the host out of a URL, for the gates that decide about one.
//!
//! A rule and a prompt are both about who is being talked to, so both need the host and neither
//! needs the rest. A destination somebody wrote down in full is the other question, and its
//! answer is the socket rather than the name, port included. Parsing is deliberately the smallest
//! thing that answers either: no dependency, no percent-decoding, and no attempt to normalise
//! anything a comparison does not depend on.
//!
//! The one property that matters is that this cannot report a host the request will not go to. A
//! parser that read `https://example.com@evil.test/` as `example.com` would hand a person the
//! wrong name to approve, so userinfo is dropped rather than mistaken for a host: everything
//! before the last `@` in the authority belongs to whoever wrote the URL, not to the destination.
//! The same holds for a socket: where the port cannot be read as the transport reads it, there is
//! none to report.

/// The host and port in `url`, lowercased, or `None` where it names no host.
///
/// For a destination somebody wrote down in full, which is a socket rather than a name: two
/// services on one machine differ by port alone, so a check that a request has not left the place
/// it was addressed to has to compare both.
pub fn authority_of(url: &str) -> Option<String> {
    // The host decides whether there is an authority at all: one that is empty or all userinfo
    // names nowhere, and a port on its own is not a destination.
    host_of(url)?;
    Some(host_and_port_of(url).to_ascii_lowercase())
}

/// The scheme, host and port a request to `url` connects to, lowercased, or `None` where it names
/// no host or no socket the transport would reach.
///
/// [`authority_of`] as written leaves an implied port off, so `http://h/` and `https://h/` read
/// the same although one connects to 80 and the other to 443. A check that a request has not left
/// the socket it was addressed to compares this instead. The scheme counts, since the server
/// answering plain text on a port is not the one answering TLS on it, and the port is the written
/// one, else the scheme's default.
///
/// A `:` after the host that is not followed by a number a port can be names no socket. The
/// transport reads no port from it and connects to the scheme's default, so the key would name a
/// socket the request never reaches. A scheme with no default port, or none at all, names none
/// either. A caller treats `None` as a socket nothing is on, the url's own included, so two urls
/// that name none are not the same socket.
pub fn socket_of(url: &str) -> Option<String> {
    let authority = authority_of(url)?;
    let (host, written) = split_port(&authority)?;
    let (scheme, _) = url.split_once("://")?;
    let scheme = scheme.trim_start().to_ascii_lowercase();
    let default = match scheme.as_str() {
        "http" => 80,
        "https" => 443,
        _ => return None,
    };
    Some(format!("{scheme}://{host}:{}", written.unwrap_or(default)))
}

/// The host of an authority as written, and the port after it, or `None` where a `:` follows the
/// host and what comes after is not a port.
///
/// A port is ASCII digits that fit a `u16`, which is what the transport reads. Leading zeros are
/// the same number. A sign is left out although the transport accepts one: a port nobody writes
/// that way is refused rather than read.
fn split_port(authority: &str) -> Option<(&str, Option<u16>)> {
    let host_end = match authority.strip_prefix('[') {
        // A bracketed IPv6 literal, whose colons are part of the address, so a port can only
        // follow its closing bracket.
        Some(rest) => rest.find(']')? + 2,
        None => authority.find(':').unwrap_or(authority.len()),
    };
    let (host, after) = authority.split_at(host_end);
    let Some(digits) = after.strip_prefix(':') else {
        return after.is_empty().then_some((host, None));
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((host, Some(digits.parse().ok()?)))
}

/// The host in `url`, lowercased, or `None` where it names none.
///
/// The port is left out. A rule naming a host means the host whichever port it answers on, and a
/// person approving one is approving who they are talking to rather than a socket.
pub fn host_of(url: &str) -> Option<String> {
    let host_and_port = host_and_port_of(url);

    let host = if let Some(rest) = host_and_port.strip_prefix('[') {
        // A bracketed IPv6 literal, whose colons are part of the address.
        rest.split_once(']').map(|(inside, _)| inside)?
    } else {
        host_and_port
            .split_once(':')
            .map(|(host, _port)| host)
            .unwrap_or(host_and_port)
    };

    if host.is_empty() {
        return None;
    }
    Some(host.to_ascii_lowercase())
}

/// The authority in `url`, with any userinfo dropped and the port left on.
fn host_and_port_of(url: &str) -> &str {
    let after_scheme = url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(url)
        .trim_start();

    // The authority ends at the first of these. Whichever comes first wins, so a `/` before a `?`
    // does not let a query string contribute to the host.
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme);

    // Userinfo is not the host. The last `@` rather than the first, since a password may hold one
    // and the destination is what follows the final separator.
    match authority.rsplit_once('@') {
        Some((_userinfo, host)) => host,
        None => authority,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two services on one machine differ by port alone, so a destination written down in full
    /// keeps it: a check on where a request went that dropped it could not tell them apart.
    #[test]
    fn an_authority_keeps_the_port_the_host_drops() {
        assert_eq!(
            authority_of("http://127.0.0.1:8931/mcp"),
            Some("127.0.0.1:8931".into())
        );
        assert_eq!(
            host_of("http://127.0.0.1:8931/mcp"),
            Some("127.0.0.1".into())
        );
        assert_eq!(
            authority_of("https://mcp.example/api"),
            Some("mcp.example".into())
        );
    }

    /// An implied port is a port: `http` and `https` on one address are two sockets, and an
    /// explicit default is the same socket as the implied one.
    #[test]
    fn a_socket_fills_in_the_port_a_scheme_implies() {
        assert_eq!(
            socket_of("http://127.0.0.1/mcp"),
            Some("http://127.0.0.1:80".into())
        );
        assert_eq!(
            socket_of("https://127.0.0.1/x"),
            Some("https://127.0.0.1:443".into())
        );
        assert_eq!(
            socket_of("HTTP://Example.com:80/x"),
            Some("http://example.com:80".into())
        );
        assert_eq!(
            socket_of("https://[::1]/x"),
            Some("https://[::1]:443".into())
        );
        assert_eq!(
            socket_of("http://[::1]:8080/x"),
            Some("http://[::1]:8080".into())
        );
        assert_eq!(socket_of("http://h:8080/x"), Some("http://h:8080".into()));
        assert_eq!(socket_of("http:///x"), None);
    }

    /// A socket is what a connection reaches, and the scheme is part of it: the server answering
    /// plain text on a port is not the one answering TLS on it, so a credential sent to one is not
    /// meant for the other, whichever port either url writes or implies.
    #[test]
    fn a_change_of_scheme_is_a_change_of_socket() {
        for (declared, hop) in [
            ("http://h:8080/a", "https://h:8080/b"),
            ("https://h:8443/a", "http://h:8443/b"),
            ("http://h/a", "https://h:80/b"),
            ("https://h/a", "http://h:443/b"),
        ] {
            let (declared_socket, hop_socket) = (socket_of(declared), socket_of(hop));
            assert!(
                declared_socket.is_some() && hop_socket.is_some(),
                "{declared} {hop}"
            );
            assert_ne!(declared_socket, hop_socket, "{declared} and {hop}");
        }
    }

    /// A written port is the number the transport connects to, so the same number spelled another
    /// way, or left out where the scheme implies it, is one socket and no question is owed for it.
    #[test]
    fn a_port_spelled_another_way_is_the_same_socket() {
        let implied = socket_of("http://h/a").expect("a socket");
        for url in [
            "http://h:80/b",
            "http://h:080/b",
            "http://h:0080/b",
            "HTTP://H:00080/c",
        ] {
            assert_eq!(socket_of(url).as_ref(), Some(&implied), "{url}");
        }
        assert_ne!(socket_of("http://h:81/a").as_ref(), Some(&implied));
        assert_eq!(
            socket_of("http://h:65535/a"),
            socket_of("http://h:065535/b")
        );
        assert!(socket_of("http://h:65535/a").is_some());
    }

    /// A `:` that is not followed by a number a port can be is not a port the transport reads: it
    /// connects to the scheme's default instead. A key built from what the url spells would name
    /// a socket the request never reaches, so such a url names none.
    #[test]
    fn a_colon_that_is_not_followed_by_a_port_names_no_socket() {
        for url in [
            "http://127.0.0.1:/a",
            "https://127.0.0.1:/b",
            "http://h:abc/a",
            "http://h:99999/a",
            "http://h:65536/a",
            "http://h:+80/a",
            "http://h:-80/a",
            "http://h: 80/a",
            "http://h:8o/a",
            "http://h:80:90/a",
            "http://[::1]:/a",
            "http://[::1]:abc/a",
            "http://[::1]:99999/a",
            "http://[::1]8080/a",
        ] {
            assert_eq!(socket_of(url), None, "{url}");
        }
    }

    /// The transport connects to `http` and `https` and takes its default from the scheme. A url
    /// with another scheme, or with none, is not one it connects to at all.
    #[test]
    fn a_scheme_the_transport_has_no_default_port_for_names_no_socket() {
        assert_eq!(socket_of("ftp://h/a"), None);
        assert_eq!(socket_of("ftp://h:21/a"), None);
        assert_eq!(socket_of("h:8080/a"), None);
    }

    /// Userinfo belongs to whoever wrote the URL, so an authority drops it for the same reason
    /// the host does: everything before the last `@` is not where the request goes.
    #[test]
    fn an_authority_is_the_destination_and_not_what_was_written_before_it() {
        assert_eq!(
            authority_of("https://example.com@evil.test:8080/x"),
            Some("evil.test:8080".into())
        );
        assert_eq!(
            authority_of("HTTPS://Example.COM:8080/x"),
            Some("example.com:8080".into())
        );
        assert_eq!(authority_of("http:///just/a/path"), None);
        assert_eq!(
            authority_of("https://[2001:db8::1]:8443/x"),
            Some("[2001:db8::1]:8443".into())
        );
    }

    #[test]
    fn the_host_is_taken_from_an_ordinary_url() {
        assert_eq!(
            host_of("https://example.com/docs"),
            Some("example.com".into())
        );
        assert_eq!(host_of("http://example.com"), Some("example.com".into()));
        assert_eq!(
            host_of("https://docs.example.com/a/b?c=d#e"),
            Some("docs.example.com".into())
        );
    }

    #[test]
    fn a_port_is_not_part_of_the_host() {
        assert_eq!(
            host_of("http://127.0.0.1:8080/path"),
            Some("127.0.0.1".into())
        );
        assert_eq!(
            host_of("https://example.com:443"),
            Some("example.com".into())
        );
    }

    /// A host is compared against rules written in lower case, and case says nothing in a URL, so
    /// the comparison must not depend on it.
    #[test]
    fn the_host_is_lowercased() {
        assert_eq!(host_of("https://EXAMPLE.com/X"), Some("example.com".into()));
    }

    /// The one that matters. Read as `example.com`, this URL would put the wrong name in front of
    /// a person and check a rule against a host the request never reaches.
    #[test]
    fn userinfo_is_not_mistaken_for_the_host() {
        assert_eq!(
            host_of("https://example.com@evil.test/path"),
            Some("evil.test".into())
        );
        assert_eq!(
            host_of("https://user:pass@example.com/path"),
            Some("example.com".into())
        );
        // A `@` inside the password does not move the boundary.
        assert_eq!(
            host_of("https://user:p@ss@real.test/"),
            Some("real.test".into())
        );
    }

    #[test]
    fn a_bracketed_address_keeps_its_colons() {
        assert_eq!(host_of("http://[::1]:8080/path"), Some("::1".into()));
        assert_eq!(
            host_of("https://[2001:db8::1]/x"),
            Some("2001:db8::1".into())
        );
    }

    #[test]
    fn a_url_with_no_host_reports_none() {
        assert_eq!(host_of("https://"), None);
        assert_eq!(host_of(""), None);
        assert_eq!(host_of("https:///just/a/path"), None);
        assert_eq!(host_of("file:///etc/passwd"), None);
    }

    /// A query or fragment before any slash must not become part of the host.
    #[test]
    fn the_authority_ends_at_the_first_delimiter() {
        assert_eq!(
            host_of("https://example.com?q=/evil.test"),
            Some("example.com".into())
        );
        assert_eq!(
            host_of("https://example.com#/evil.test"),
            Some("example.com".into())
        );
    }
}
