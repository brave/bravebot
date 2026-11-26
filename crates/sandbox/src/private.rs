//! A directory this account alone may enter, and a name nobody can predict, for the files and
//! directories a program makes for itself in a place a confined stage cannot write.
//!
//! Both are what a caller needs before it creates something by name and opens it again: the
//! directory is refused where another account or a link stands in its place, and the name is read
//! from the operating system's random source. Where the directory is chosen is the caller's: the
//! system temporary directory is a place every stage of the account may write, so a caller that
//! needs the file to stay what it wrote places it elsewhere.

use std::io::Read;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

/// `directory`, made if absent and set to mode 0700 whether it was made now or before. Refused
/// where the path is a link or a directory another account owns, since a mode on those says
/// nothing about who may replace what is inside.
pub fn owner_only_directory(directory: &Path) -> std::io::Result<()> {
    crate::policy::make_a_directory(directory)?;
    let found = std::fs::symlink_metadata(directory)?;
    if !found.is_dir() {
        return Err(std::io::Error::other(format!(
            "`{}` is not a directory",
            directory.display()
        )));
    }
    if found.uid() != rustix::process::geteuid().as_raw() {
        return Err(std::io::Error::other(format!(
            "`{}` is not owned by this account",
            directory.display()
        )));
    }
    std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))
}

/// Thirty-two lower-case hexadecimal digits (128 bits) read from `/dev/urandom`, which the
/// operating system fills from its own randomness. No other source is used, because a stage can
/// work out a name made from the process id, a counter or the time and create that file first.
pub fn random_name() -> std::io::Result<String> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    let mut name = String::with_capacity(32);
    for byte in bytes {
        name.push(char::from(HEX[usize::from(byte >> 4)]));
        name.push(char::from(HEX[usize::from(byte & 0xf)]));
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory under the crate's scratch root, emptied first and removed when this is dropped,
    /// so a test that fails does not leave what it made for the next run to measure.
    struct Emptied(std::path::PathBuf);

    impl Emptied {
        #[expect(clippy::expect_used, reason = "a test fails by panicking")]
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the directory is usually absent, and a failed removal shows at the create below"
        )]
        fn new(name: &str) -> Self {
            let path = crate::testutil::scratch_dir(name);
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a directory for the test");
            Self(path)
        }
    }

    impl Drop for Emptied {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "nobody is left to tell when a test has ended"
        )]
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn mode_of(path: &Path) -> u32 {
        std::fs::symlink_metadata(path)
            .expect("exists")
            .permissions()
            .mode()
            & 0o777
    }

    /// A link where the directory should be is refused, and nothing is made or changed where it
    /// leads. A mode set through the link would change a directory this process did not make, and a
    /// directory made through it would be in a place whose owner is not known.
    ///
    /// The regression it rejects is a check of the path that follows links (`metadata` in place of
    /// `symlink_metadata`), which accepts the link as the directory it names and narrows that one.
    #[test]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn a_link_in_place_of_the_directory_is_refused_and_leads_nowhere_new() {
        let root = Emptied::new("private-linked-directory");
        let elsewhere = root.0.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).expect("create where the link leads");
        std::fs::set_permissions(&elsewhere, std::fs::Permissions::from_mode(0o755))
            .expect("chmod");
        let link = root.0.join("link");
        std::os::unix::fs::symlink(&elsewhere, &link).expect("link");

        let refused = owner_only_directory(&link);

        assert!(
            refused
                .as_ref()
                .is_err_and(|error| error.to_string().contains("is not a directory")),
            "a link was taken for a directory, or refused for another reason: {refused:?}"
        );
        assert_eq!(
            mode_of(&elsewhere),
            0o755,
            "the directory the link leads to was narrowed"
        );
        assert!(
            std::fs::read_dir(&elsewhere)
                .expect("read where the link leads")
                .next()
                .is_none(),
            "something was made where the link leads"
        );
    }

    /// A directory is mode 0700 whether it was made now, with the directories above it that were
    /// absent, or was there already at the umask of whoever made it.
    ///
    /// The regression it rejects is a call that creates and does not set the mode
    /// (`create_dir_all` alone), which leaves an existing directory as open as it was.
    #[test]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn a_directory_made_now_or_found_open_ends_at_mode_0700() {
        let root = Emptied::new("private-narrowed-directory");
        let fresh = root.0.join("above").join("fresh");
        owner_only_directory(&fresh).expect("made");
        assert_eq!(mode_of(&fresh), 0o700, "a directory made now is open");

        let open = root.0.join("open");
        std::fs::create_dir_all(&open).expect("as an earlier run left it");
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o777)).expect("chmod");
        owner_only_directory(&open).expect("narrowed");
        assert_eq!(mode_of(&open), 0o700, "an open directory was left open");
    }

    /// The name is 32 lower-case hexadecimal digits that no program able to write where it is used
    /// can work out, so none can create the name first.
    ///
    /// The regression it rejects is a name built from the process id, a clock reading or a counter
    /// and padded to the same length: each is a value a stage computes. Read as two 64-bit halves,
    /// such a name has a half that is zero, constant or a few steps from the same half of the name
    /// before it, and a random name has none of those but with a probability of about 2^-28. The
    /// format alone passes a counter padded to 32 digits.
    #[test]
    #[expect(clippy::expect_used, reason = "a test fails by panicking")]
    fn a_random_name_is_thirty_two_lower_case_hexadecimal_digits_and_not_one_to_work_out() {
        let halves: Vec<(u64, u64)> = (0..8)
            .map(|_| {
                let name = random_name().expect("a name");
                assert_eq!(name.len(), 32, "{name}");
                assert!(
                    name.bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
                    "{name} is not lower-case hexadecimal"
                );
                let (first, second) = name.split_at(16);
                (
                    u64::from_str_radix(first, 16).expect("hexadecimal"),
                    u64::from_str_radix(second, 16).expect("hexadecimal"),
                )
            })
            .collect();
        for pair in halves.windows(2) {
            let (first, second) = (pair[0], pair[1]);
            assert!(
                first.0.abs_diff(second.0) > 1 << 32,
                "two names in a row share their first half or step it: {first:x?} {second:x?}"
            );
            assert!(
                first.1.abs_diff(second.1) > 1 << 32,
                "two names in a row share their second half or step it: {first:x?} {second:x?}"
            );
        }
    }

    /// A directory another account owns is refused by the owner check, since its owner may replace
    /// what is inside whatever its mode. The root directory stands for one, as an account that is
    /// not root cannot make a directory owned by another. The refusal is by the owner check: a
    /// change of the mode would also fail for this account, with another message.
    ///
    /// The regression it rejects is an owner check that is dropped, which leaves a directory
    /// another account made as the place a session index or a policy file is created in. Where the
    /// tests run as root, which owns the directory used as another's, the test returns without
    /// asserting and the owner check is covered by reading only.
    #[test]
    fn a_directory_another_account_owns_is_refused() {
        if rustix::process::geteuid().is_root() {
            eprintln!("SKIPPED (running as root, which owns the directory used as another's)");
            return;
        }
        let refused = owner_only_directory(Path::new("/"));
        assert!(
            refused
                .as_ref()
                .is_err_and(|error| error.to_string().contains("not owned by this account")),
            "refused for another reason, or not refused: {refused:?}"
        );
    }
}
