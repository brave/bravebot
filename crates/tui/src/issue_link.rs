//! `#<number>` in an assistant reply, linked to the issue or pull request of that number on the
//! repository the checkout's `origin` remote names (VIEW-32).
//!
//! The number comes from released text and the repository from the checkout's own configuration,
//! which is workspace content. Both are for display only: a link is something a person chooses to
//! open, and nothing here is read by a decision. A configuration can therefore choose which
//! `github.com` repository a number links to and nothing else, because the host is fixed, each
//! name is held to the characters GitHub allows in it, and [`crate::hyperlink::link`] writes
//! nothing for an address with anything but printable ASCII in it.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use std::path::Path;

/// The host the only links are made for.
const HOST: &str = "github.com";

/// The most digits a number may have, which is more than a repository's issues have reached.
pub const MAX_DIGITS: usize = 9;

/// How the matcher marks a number it found, for [`link_references`] to find on the screen.
///
/// The transcript is wrapped by the widget that draws it, so where a number lands is known only
/// once it is drawn, and the one thing the drawn cell keeps of what the matcher decided is its
/// style. Nothing else in the transcript is underlined.
pub const MARK: Modifier = Modifier::UNDERLINED;

/// A GitHub repository: the owner and the name its address spells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repository {
    owner: String,
    name: String,
}

impl Repository {
    /// The repository the `origin` of the checkout holding `directory` names, where it is on
    /// GitHub.
    pub fn of(directory: &Path) -> Option<Self> {
        Self::from_remote(&bravebot_session::sessions::origin_of(directory)?)
    }

    /// The repository a remote's address names: `https://github.com/{owner}/{name}`, with or
    /// without `.git` after the name, and the `ssh://` and `git@github.com:{owner}/{name}` spellings
    /// of the same. `None` for any other host, which is compared as a whole, and for an owner or
    /// name holding a character GitHub does not allow in it.
    pub fn from_remote(address: &str) -> Option<Self> {
        let (host, path) = split(address)?;
        if !host.eq_ignore_ascii_case(HOST) {
            return None;
        }
        let mut parts = path.trim_matches('/').split('/');
        let (owner, name) = (parts.next()?, parts.next()?);
        if parts.next().is_some() {
            return None;
        }
        let name = name.strip_suffix(".git").unwrap_or(name);
        let owner_spelled = !owner.is_empty()
            && owner
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
        let name_spelled = !matches!(name, "" | "." | "..")
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
        (owner_spelled && name_spelled).then(|| Self {
            owner: owner.to_string(),
            name: name.to_string(),
        })
    }

    /// The address of issue or pull request `number`. GitHub sends `/issues/{n}` to `/pull/{n}`
    /// where `{n}` is a pull request, so one form covers both.
    pub fn address(&self, number: &str) -> Option<String> {
        is_number(number).then(|| {
            format!(
                "https://{HOST}/{}/{}/issues/{number}",
                self.owner, self.name
            )
        })
    }
}

/// Whether `text` is an issue or pull request number: one to [`MAX_DIGITS`] digits, the first not
/// a zero, since neither `#0` nor `#007` is the number of anything.
pub fn is_number(text: &str) -> bool {
    (1..=MAX_DIGITS).contains(&text.len())
        && text.bytes().all(|byte| byte.is_ascii_digit())
        && !text.starts_with('0')
}

/// The schemes a remote's address may begin with.
const SCHEMES: [&str; 6] = ["https", "http", "ssh", "git", "git+ssh", "ssh+git"];

/// The host and the path of a remote's address, the user and the port left out.
fn split(address: &str) -> Option<(&str, &str)> {
    let (authority, path) = match address.split_once("://") {
        Some((scheme, rest)) => {
            if !SCHEMES
                .iter()
                .any(|known| known.eq_ignore_ascii_case(scheme))
            {
                return None;
            }
            rest.split_once('/')?
        }
        // `[user@]host:path`, which has no `/` before its first `:`.
        None => {
            let (before, path) = address.split_once(':')?;
            if before.contains('/') {
                return None;
            }
            (before, path)
        }
    };
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    match host.split_once(':') {
        Some((host, port))
            if !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            Some((host, path))
        }
        Some(_) => None,
        None => Some((host, path)),
    }
}

/// The character in the cell at `(x, y)` where the matcher marked it ([`MARK`]).
fn marked(buffer: &Buffer, x: u16, y: u16) -> Option<&str> {
    let cell = buffer.cell((x, y))?;
    cell.style()
        .add_modifier
        .contains(MARK)
        .then(|| cell.symbol())
}

fn marked_digit(buffer: &Buffer, x: u16, y: u16) -> Option<char> {
    let mut symbol = marked(buffer, x, y)?.chars();
    symbol
        .next()
        .filter(char::is_ascii_digit)
        .filter(|_| symbol.next().is_none())
}

/// Make each number the matcher marked in `area` a link to its issue or pull request.
///
/// Read off the cells, which is where the transcript's wrapping has put each number. A number the
/// widget split across two rows is not linked, since neither half names it.
pub fn link_references(buffer: &mut Buffer, area: Rect, repository: &Repository) {
    let area = area.intersection(buffer.area);
    for y in area.top()..area.bottom() {
        let mut x = area.left();
        while x < area.right() {
            if marked(buffer, x, y) != Some("#") {
                x += 1;
                continue;
            }
            let mut digits = String::new();
            let mut end = x + 1;
            while end < area.right()
                && let Some(digit) = marked_digit(buffer, end, y)
            {
                digits.push(digit);
                end += 1;
            }
            let split = end == area.right()
                && y + 1 < area.bottom()
                && marked_digit(buffer, area.left(), y + 1).is_some();
            if !split && let Some(address) = repository.address(&digits) {
                crate::hyperlink::link(buffer, x, y, end - x, &address);
            }
            x = end;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Style;

    fn repository(address: &str) -> Option<(String, String)> {
        Repository::from_remote(address).map(|found| (found.owner, found.name))
    }

    fn brave() -> Option<(String, String)> {
        Some(("brave".to_string(), "bravebot".to_string()))
    }

    /// The spellings `git clone` writes for a GitHub repository, and each of them names the same
    /// one, so a checkout made one way links as a checkout made another.
    #[test]
    fn each_spelling_of_a_github_remote_names_the_repository() {
        for address in [
            "https://github.com/brave/bravebot",
            "https://github.com/brave/bravebot.git",
            "https://github.com/brave/bravebot/",
            "https://GitHub.com/brave/bravebot.git",
            "https://user:token@github.com/brave/bravebot.git",
            "http://github.com/brave/bravebot",
            "git@github.com:brave/bravebot.git",
            "git@github.com:brave/bravebot",
            "github.com:brave/bravebot.git",
            "git@github.com:/brave/bravebot.git",
            "ssh://git@github.com/brave/bravebot.git",
            "ssh://git@github.com:22/brave/bravebot.git",
            "git://github.com/brave/bravebot.git",
            "git+ssh://git@github.com/brave/bravebot.git",
        ] {
            assert_eq!(repository(address), brave(), "{address}");
        }
    }

    /// The host is the whole of what makes an address GitHub's. A name that holds `github.com`, a
    /// user that is `github.com` and a port that is not one are other hosts.
    #[test]
    fn only_the_host_github_com_names_a_repository() {
        for address in [
            "https://gitlab.com/brave/bravebot",
            "https://github.com.evil.test/brave/bravebot",
            "https://evilgithub.com/brave/bravebot",
            "https://www.github.com/brave/bravebot",
            "https://github.com@evil.test/brave/bravebot",
            "https://github.com:evil/brave/bravebot",
            "git@github.com.evil.test:brave/bravebot.git",
            "git@gist.github.com:brave/bravebot.git",
            "ftp://github.com/brave/bravebot",
            "file:///srv/github.com/brave/bravebot",
            "/srv/github.com/brave/bravebot",
            "../bravebot",
            "",
        ] {
            assert_eq!(repository(address), None, "{address}");
        }
    }

    /// The owner and the name go into an address a terminal is sent, so each holds only what
    /// GitHub allows in it, and a path with more or fewer parts than an owner and a name is not
    /// a repository's.
    #[test]
    fn an_owner_or_name_outside_the_characters_github_allows_names_nothing() {
        for address in [
            "https://github.com/br ave/bravebot",
            "https://github.com/brave/brave bot",
            "https://github.com/br_ave/bravebot",
            "https://github.com/br.ave/bravebot",
            "https://github.com/brave/brave\u{1b}]8;;bot",
            "https://github.com/brave/brave\u{7}bot",
            "https://github.com/brave/brave\u{e9}bot",
            "https://github.com/brave/brave%20bot",
            "https://github.com/brave/..",
            "https://github.com/brave/.",
            "https://github.com/brave/.git",
            "https://github.com/../bravebot",
            "https://github.com//bravebot",
            "https://github.com/brave",
            "https://github.com/brave/bravebot/issues",
            "https://github.com/",
            "git@github.com:brave",
        ] {
            assert_eq!(repository(address), None, "{address:?}");
        }
        assert_eq!(
            repository("https://github.com/a-b/c_d.e-f.git"),
            Some(("a-b".to_string(), "c_d.e-f".to_string()))
        );
    }

    /// The number is the one the text gave, and a number that is not one makes no address.
    #[test]
    fn an_address_is_made_for_a_number_and_for_nothing_else() {
        let found = Repository::from_remote("git@github.com:brave/bravebot.git").expect("github");
        assert_eq!(
            found.address("2879").as_deref(),
            Some("https://github.com/brave/bravebot/issues/2879")
        );
        assert_eq!(
            found.address("123456789").as_deref(),
            Some("https://github.com/brave/bravebot/issues/123456789")
        );
        for number in [
            "",
            "0",
            "007",
            "1234567890",
            "12a",
            "-1",
            "1/../2",
            "1\u{1b}",
            "\u{661}",
        ] {
            assert_eq!(found.address(number), None, "{number:?}");
        }
    }

    fn screen(rows: &[&str]) -> Buffer {
        Buffer::with_lines(rows.iter().copied())
    }

    /// Mark the cells from `(x, y)` as the matcher marks a number.
    fn mark(buffer: &mut Buffer, x: u16, y: u16, width: u16) {
        for column in x..x + width {
            buffer[(column, y)].set_style(Style::default().add_modifier(MARK));
        }
    }

    fn linked(buffer: &Buffer, x: u16, y: u16) -> bool {
        crate::hyperlink::shown(buffer[(x, y)].symbol()) != buffer[(x, y)].symbol()
    }

    fn the_repository() -> Repository {
        Repository::from_remote("https://github.com/brave/bravebot").expect("github")
    }

    /// The cells to link are found by what is on the screen, so a number drawn on a row other
    /// than the one its text began on, or after other text, is linked where it is.
    #[test]
    fn a_marked_number_is_linked_wherever_it_was_drawn() {
        let mut buffer = screen(&["see the fix in", "  #12 and #345."]);
        mark(&mut buffer, 2, 1, 3);
        mark(&mut buffer, 10, 1, 4);
        let area = buffer.area;
        link_references(&mut buffer, area, &the_repository());

        let linked_cells: Vec<(u16, u16)> = (0..2)
            .flat_map(|y| (0..15).map(move |x| (x, y)))
            .filter(|&(x, y)| linked(&buffer, x, y))
            .collect();
        assert_eq!(
            linked_cells,
            [(2, 1), (3, 1), (4, 1), (10, 1), (11, 1), (12, 1), (13, 1)]
        );
        assert_eq!(
            buffer[(3, 1)].symbol(),
            "\x1b]8;;https://github.com/brave/bravebot/issues/12\x1b\\1\x1b]8;;\x1b\\"
        );
        assert!(
            buffer[(11, 1)]
                .symbol()
                .contains("/brave/bravebot/issues/345")
        );
    }

    /// The marking is what says the matcher found the number, so the same characters drawn by
    /// anything else are left as they are.
    #[test]
    fn a_number_the_matcher_did_not_mark_is_not_linked() {
        let mut buffer = screen(&["see #12 now"]);
        let area = buffer.area;
        link_references(&mut buffer, area, &the_repository());
        assert_eq!(buffer, screen(&["see #12 now"]));

        // The digits alone, or a hash alone, are not a number.
        let mut buffer = screen(&["#12 #"]);
        mark(&mut buffer, 1, 0, 2);
        mark(&mut buffer, 4, 0, 1);
        let area = buffer.area;
        link_references(&mut buffer, area, &the_repository());
        assert!((0..5).all(|x| !linked(&buffer, x, 0)));
    }

    /// A number the widget split has two halves, and linking the first would name another
    /// issue than the one written.
    #[test]
    fn a_number_split_across_two_rows_is_not_linked() {
        let mut buffer = screen(&["see #1234", "56789 now"]);
        mark(&mut buffer, 4, 0, 5);
        mark(&mut buffer, 0, 1, 5);
        let area = buffer.area;
        link_references(&mut buffer, area, &the_repository());
        assert!((0..9).all(|x| !linked(&buffer, x, 0) && !linked(&buffer, x, 1)));

        // One that ends the row and is followed by something else is whole.
        let mut buffer = screen(&["see #1234", "now"]);
        mark(&mut buffer, 4, 0, 5);
        let area = buffer.area;
        link_references(&mut buffer, area, &the_repository());
        assert!((4..9).all(|x| linked(&buffer, x, 0)));
    }

    /// The area bounds the pass, so a number outside it, which another widget drew, is left alone.
    #[test]
    fn a_number_outside_the_area_is_not_linked() {
        let mut buffer = screen(&["#1 #2 #3"]);
        mark(&mut buffer, 0, 0, 2);
        mark(&mut buffer, 3, 0, 2);
        mark(&mut buffer, 6, 0, 2);
        link_references(&mut buffer, Rect::new(3, 0, 2, 1), &the_repository());
        let linked_cells: Vec<u16> = (0..8).filter(|&x| linked(&buffer, x, 0)).collect();
        assert_eq!(linked_cells, [3, 4]);
    }

    /// More digits than a number has are not a number, so the run is left as it was drawn.
    #[test]
    fn a_run_of_digits_longer_than_a_number_is_not_linked() {
        let mut buffer = screen(&["#1234567890"]);
        mark(&mut buffer, 0, 0, 11);
        let area = buffer.area;
        link_references(&mut buffer, area, &the_repository());
        assert!((0..11).all(|x| !linked(&buffer, x, 0)));
    }
}
