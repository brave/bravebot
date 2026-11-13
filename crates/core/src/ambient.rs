//! Authority a line reaches rather than holds.
//!
//! A container daemon's socket, a command-line tool that is already logged in, the ssh agent and
//! the metadata service of the machine this runs on are all reached instead of held. Nothing is
//! handed over when one is used, nobody is asked at the moment of use, and this process could not
//! take the access back afterwards, so there is no custody to rank and no bound to enforce: the
//! credential is on no tier at all. What is left is that the person granting the capability
//! should not grant it thinking they granted less, which means naming which authority a grant
//! reaches where the grant is given, and recording it where it is spent.
//!
//! # This names, and refuses nothing
//!
//! Nothing here decides anything. A line that reaches a container daemon runs exactly as it would
//! if this module did not exist; what changes is that the person approving it was told which
//! authority they were handing over rather than only that the command is unsandboxed.
//!
//! So the direction to fail in is the reverse of [`crate::pure`]'s. There a name wrongly
//! recognised trusts a program nobody audited, so the table is an allowlist and anything outside
//! it proves nothing. Here a name wrongly recognised costs a sentence on a prompt, and a name
//! missed costs the whole of what is owed, so the table is read generously: any occurrence of a
//! word from it is enough, and `git log --grep push` naming git on the prompt is the cheap
//! mistake. None of this is a boundary. Whatever the table does not recognise is unsandboxed
//! still, and the line saying so is said every time regardless.
//!
//! # A name is enough, and the resolved file is another name
//!
//! [`crate::pure`] insists on the resolved program because a proof about `grep` must not follow
//! the name onto a different implementation. The question here is not what the program does but
//! which authority it reaches, and a program calling itself `docker` in a directory on `$PATH`
//! says the same thing about that as `/usr/bin/docker` does. Both spellings are read, and either
//! one naming an entry is enough.
//!
//! # What may be kept
//!
//! A record holds no content ([TRACE-2]), and an argument holds a URL's path as readily as a
//! program's name. So what is reported is the word from the table that matched and never the
//! argument it was found in: every value this module hands back is its own.
//!
//! [TRACE-2]: ../../../docs/specs/trace.md

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::command::Plan;

/// An authority nothing here has custody of and nobody can refuse a use of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authority {
    /// A container daemon, reached over a socket that grants whatever the daemon can do.
    ContainerDaemon,
    /// A tool holding a login of its own, which acts as the person without asking them.
    LoggedInTool,
    /// The ssh agent, which signs with keys it never hands over.
    AgentSocket,
    /// The metadata service of the machine this runs on, which issues the credentials of the
    /// role it runs as to anything that can reach the address.
    MetadataService,
}

impl Authority {
    /// The name a program reads this by.
    ///
    /// Not the sentence a person reads, which is in the message catalog and changes with their
    /// language. A record is read back by whoever is accounting for a session, months later and
    /// possibly in another language, which is the same reason
    /// [`crate::event::Principle::name`] is not localised either.
    pub fn name(&self) -> &'static str {
        match self {
            Self::ContainerDaemon => "container-daemon",
            Self::LoggedInTool => "logged-in-tool",
            Self::AgentSocket => "agent-socket",
            Self::MetadataService => "metadata-service",
        }
    }

    /// The authority a [`Authority::name`] names, for a front end that was sent the name.
    pub fn from_name(name: &str) -> Option<Self> {
        [
            Self::ContainerDaemon,
            Self::LoggedInTool,
            Self::AgentSocket,
            Self::MetadataService,
        ]
        .into_iter()
        .find(|authority| authority.name() == name)
    }
}

/// One authority something reaches, and the word that named it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spent {
    /// What is reached.
    pub authority: Authority,
    /// The word from this module's own tables that matched: a program, an address, a socket or a
    /// variable.
    ///
    /// `'static` because it is always one of those, never a byte of what was matched against. It
    /// goes on a prompt and into the record, and a record holds no content.
    pub named: &'static str,
}

/// A program that reaches an ambient authority.
struct Reaching {
    /// The program's name, as the line spells it or as the file it resolved to is called.
    program: &'static str,
    /// What running it reaches.
    authority: Authority,
    /// The arguments that spend it, where only some calls do. Empty means every call does.
    ///
    /// Matched anywhere in the argument list rather than at the position a subcommand occupies,
    /// because working out that position means knowing which options take a value, which is the
    /// option-parsing race [`crate::pure`] exists to avoid. The two modules answer it in opposite
    /// directions: there a miscount admits a call nobody audited, and here it names a tool on a
    /// prompt that was not going to spend anything.
    spends: &'static [&'static str],
}

/// Every program recognised, with what it reaches.
///
/// Each entry is a tool whose stored credential the credential inventory already lists, so the
/// list is not an invention of this module: it is the clients of the things in that table that
/// act on a remote service under a login nobody is asked for again.
///
/// A registry client and a database client are left out where every call would match. `npm` and
/// `cargo` spend a token on the few commands that publish and nothing on the hundreds that build,
/// and a line said on every `npm ci` is a line people learn to skip past, which costs more than
/// the case it covers. They are here under those commands and no others, which is what `spends`
/// is for.
const REACHING: &[Reaching] = &[
    // A container daemon runs a container as root on the host and mounts whatever it is told to,
    // so reaching its socket is reaching the machine.
    Reaching {
        program: "docker",
        authority: Authority::ContainerDaemon,
        spends: &[],
    },
    Reaching {
        program: "docker-compose",
        authority: Authority::ContainerDaemon,
        spends: &[],
    },
    Reaching {
        program: "podman",
        authority: Authority::ContainerDaemon,
        spends: &[],
    },
    Reaching {
        program: "nerdctl",
        authority: Authority::ContainerDaemon,
        spends: &[],
    },
    // Each of these reads a credential of its own from the home directory and acts as the person
    // with it. Nothing asks them for it and nothing here can take it back.
    Reaching {
        program: "gh",
        authority: Authority::LoggedInTool,
        spends: &[],
    },
    Reaching {
        program: "glab",
        authority: Authority::LoggedInTool,
        spends: &[],
    },
    Reaching {
        program: "aws",
        authority: Authority::LoggedInTool,
        spends: &[],
    },
    Reaching {
        program: "gcloud",
        authority: Authority::LoggedInTool,
        spends: &[],
    },
    Reaching {
        program: "gsutil",
        authority: Authority::LoggedInTool,
        spends: &[],
    },
    Reaching {
        program: "az",
        authority: Authority::LoggedInTool,
        spends: &[],
    },
    Reaching {
        program: "kubectl",
        authority: Authority::LoggedInTool,
        spends: &[],
    },
    Reaching {
        program: "helm",
        authority: Authority::LoggedInTool,
        spends: &[],
    },
    Reaching {
        program: "doctl",
        authority: Authority::LoggedInTool,
        spends: &[],
    },
    Reaching {
        program: "flyctl",
        authority: Authority::LoggedInTool,
        spends: &[],
    },
    Reaching {
        program: "heroku",
        authority: Authority::LoggedInTool,
        spends: &[],
    },
    Reaching {
        program: "vercel",
        authority: Authority::LoggedInTool,
        spends: &[],
    },
    Reaching {
        program: "netlify",
        authority: Authority::LoggedInTool,
        spends: &[],
    },
    Reaching {
        program: "supabase",
        authority: Authority::LoggedInTool,
        spends: &[],
    },
    Reaching {
        program: "terraform",
        authority: Authority::LoggedInTool,
        spends: &[],
    },
    Reaching {
        program: "pulumi",
        authority: Authority::LoggedInTool,
        spends: &[],
    },
    Reaching {
        program: "vault",
        authority: Authority::LoggedInTool,
        spends: &[],
    },
    // A remote is reached with whatever the credential helper holds or whatever the agent will
    // sign, and which of the two it is depends on the remote rather than on the line. Only the
    // commands that reach one: the log and the diff spend nothing.
    Reaching {
        program: "git",
        authority: Authority::LoggedInTool,
        spends: &["push", "pull", "fetch", "clone", "ls-remote"],
    },
    Reaching {
        program: "npm",
        authority: Authority::LoggedInTool,
        spends: &[
            "publish",
            "unpublish",
            "owner",
            "access",
            "token",
            "deprecate",
        ],
    },
    Reaching {
        program: "cargo",
        authority: Authority::LoggedInTool,
        spends: &["publish", "owner", "yank", "login"],
    },
    // The agent signs for a key it holds without the key or anything derived from it ever
    // arriving here, which is the case with nothing to rank at all.
    Reaching {
        program: "ssh",
        authority: Authority::AgentSocket,
        spends: &[],
    },
    Reaching {
        program: "scp",
        authority: Authority::AgentSocket,
        spends: &[],
    },
    Reaching {
        program: "sftp",
        authority: Authority::AgentSocket,
        spends: &[],
    },
    Reaching {
        program: "ssh-add",
        authority: Authority::AgentSocket,
        spends: &[],
    },
];

/// Text that names an authority wherever a line writes it.
///
/// A daemon reached with `curl --unix-socket /var/run/docker.sock` runs no program from the table
/// above, and the socket is the thing being reached either way.
const NAMED_IN_A_LINE: &[(&str, Authority)] = &[
    ("docker.sock", Authority::ContainerDaemon),
    ("podman.sock", Authority::ContainerDaemon),
    ("containerd.sock", Authority::ContainerDaemon),
    ("SSH_AUTH_SOCK", Authority::AgentSocket),
];

/// The addresses a cloud instance's metadata service answers on.
///
/// Link-local and unroutable, so nothing outside the machine can reach one and nothing inside it
/// is asked for a credential before it answers. A process that can open a socket has the role's
/// credentials, which is the whole of the arrangement.
const METADATA: &[&str] = &[
    // EC2, GCE, Azure, DigitalOcean and OpenStack all answer here.
    "169.254.169.254",
    // The same service over IPv6 on EC2.
    "fd00:ec2::254",
    // An ECS task's own role, which is a different address from the instance's.
    "169.254.170.2",
    // Alibaba Cloud.
    "100.100.100.200",
    // The name GCE resolves for the address above, and the one its documentation uses.
    "metadata.google.internal",
];

/// Every ambient authority a plan reaches, in the order the line names them.
///
/// One entry per authority and word: a line naming two logged-in tools is two entries, because a
/// person granting it is granting both, and a line naming one twice is one.
pub fn spent_by(plan: &Plan) -> Vec<Spent> {
    let mut spent: Vec<Spent> = Vec::new();
    let mut found = |authority, named| {
        let entry = Spent { authority, named };
        if !spent.contains(&entry) {
            spent.push(entry);
        }
    };
    for step in plan.steps() {
        // Destructured, so a field added to a step stops the build here and somebody decides
        // whether an authority can be named in it. That is the rule a key is held to, and it is
        // worth as much of a question asked of every field: the next way to reach a daemon would
        // otherwise be added to the step and silently not read.
        //
        // The routes are what the line opens rather than what it reaches, so a redirection names
        // nothing on this account: a destination is the write set's question and is drawn on the
        // same prompt, above this.
        //
        // The path a step is started by ends in the name the line used, which is read already. A
        // multi-call binary started through a link called `docker` is caught by that name.
        let crate::command::Step {
            program,
            resolved,
            started_as: _,
            args,
            // Which words a pattern produced changes nothing a program reaches.
            matched: _,
            environment,
            expansions,
            routes: _,
        } = step;
        // A program a wrapper runs reaches what it reaches without one, so the words left after
        // each leading wrapper are read too, each program with the arguments it is given and
        // not the wrapper's. The table of wrappers is the one a deny or ask rule is matched
        // through (PERM-20); the file a wrapper resolved to is not looked through, and a
        // program that only runs another by another way (`xargs`, `npx`, `docker exec`) is read
        // by its own name alone.
        let argv: Vec<&str> = std::iter::once(program.as_str())
            .chain(args.iter().map(String::as_str))
            .collect();
        let resolved = resolved.to_string_lossy();
        let mut called: Vec<(&str, &[&str])> = vec![
            (program.as_str(), &argv[1..]),
            (resolved.as_ref(), &argv[1..]),
        ];
        let inner = crate::permissions::wrapped_commands(&argv);
        called.extend(inner.iter().map(|words| (words[0], &words[1..])));
        for (spelling, given) in called {
            // Folded as a deny or ask rule folds the program word (PERM-5), so a spelling the
            // volume opens as the program is read as the program.
            let name = crate::trust::fold_simple(program_name(spelling));
            for entry in REACHING {
                let named = entry.program == name;
                let spending =
                    entry.spends.is_empty() || given.iter().any(|arg| entry.spends.contains(arg));
                if named && spending {
                    found(entry.authority, entry.program);
                }
            }
        }
        // Every other string the step carries. An assignment written in front of a program is
        // read as well as its arguments: `DOCKER_HOST` and `SSH_AUTH_SOCK` are said there rather
        // than in an argument, and the point of an assignment is that it decides what the program
        // reaches.
        // What an expansion names is read as an assignment is: its name, and the text and the
        // variable it is written with. The value is not known here and never is; a command inside
        // one is a step of the plan and read in its turn.
        let expanded: Vec<&str> = expansions
            .iter()
            .flat_map(|expansion| {
                std::iter::once(expansion.name.as_str()).chain(expansion.parts.iter().filter_map(
                    |part| match part {
                        crate::command::ValuePart::Text(text) => Some(text.as_str()),
                        crate::command::ValuePart::Variable(name) => Some(name.as_str()),
                        crate::command::ValuePart::Substitution { .. } => None,
                    },
                ))
            })
            .collect();
        let written = args.iter().map(String::as_str).chain(expanded).chain(
            environment
                .iter()
                .flat_map(|(name, value)| [name.as_str(), value.as_str()]),
        );
        for text in written {
            for (token, authority) in NAMED_IN_A_LINE {
                if text.contains(token) {
                    found(*authority, token);
                }
            }
            for address in METADATA {
                if text.contains(address) {
                    found(Authority::MetadataService, address);
                }
            }
        }
    }
    spent
}

/// One dotted part of an address spelling, as a number of the bytes it holds.
///
/// A leading `0x` is hexadecimal and a leading `0` octal, which are how `getaddrinfo` reads a
/// part, so `0xA9` and `0251` are both 169 here as well.
fn part_bytes(part: &str, bytes: usize) -> Option<u32> {
    // `from_str_radix` accepts a leading sign, which a resolver does not.
    if part.starts_with(['+', '-']) || part.strip_prefix("0x").is_some_and(|r| r.starts_with('+')) {
        return None;
    }
    let value = if let Some(rest) = part.strip_prefix("0x").or_else(|| part.strip_prefix("0X")) {
        u32::from_str_radix(rest, 16).ok()?
    } else if part.len() > 1 && part.starts_with('0') {
        u32::from_str_radix(&part[1..], 8).ok()?
    } else if part.is_empty() {
        return None;
    } else {
        part.parse::<u32>().ok()?
    };
    // The part names however many bytes are given it, which is one for everything but the last.
    let limit = 1u64 << (8 * bytes);
    ((value as u64) < limit).then_some(value)
}

/// The address a host spelling denotes, when that spelling is a number rather than a name.
///
/// `std` reads `169.254.169.254` but not the other spellings of the same number, and name
/// resolution reads them all: a dword such as `2852039166`, a hexadecimal one such as
/// `0xA9FEA9FE`, and dotted forms whose last part carries the bytes the earlier parts leave
/// over, such as `169.254.43518`. The POSIX rules are the ones such a resolver holds to: one to
/// four parts, each earlier part one byte, and the last part however many bytes are left.
fn parse_ipv4(host: &str) -> Option<Ipv4Addr> {
    let parts = host.split('.').collect::<Vec<_>>();
    let (last, earlier) = parts.split_last()?;
    if parts.len() > 4 || earlier.iter().any(|part| part.is_empty()) {
        return None;
    }
    let earlier = earlier
        .iter()
        .map(|part| part_bytes(part, 1))
        .collect::<Option<Vec<_>>>()?;
    let last = part_bytes(last, 5 - parts.len())?;
    let mut octets = [0u8; 4];
    for (index, value) in earlier.iter().enumerate() {
        octets[index] = *value as u8;
    }
    // The last part holds the most significant of its bytes first, as `169.254.43518` being
    // 169.254.169.254 shows.
    for (index, byte) in last.to_be_bytes()[4 - (5 - parts.len())..]
        .iter()
        .enumerate()
    {
        octets[earlier.len() + index] = *byte;
    }
    Some(Ipv4Addr::from(octets))
}

/// The address a host spelling denotes, whether that spelling is `std`'s or a resolver's.
///
/// An address that only `getaddrinfo` reads is the same host as the one `std` reads, so a host
/// is parsed here before it is compared, and a spelling of the metadata service's address is
/// named as the service however the number was written.
///
/// An IPv4 address written inside an IPv6 one, as `::ffff:a.b.c.d` or in `64:ff9b::/96`, is
/// that IPv4 address. A connection to either goes to the IPv4 address, and the fetch and the
/// sandbox proxy classify it as the IPv4 address too.
fn parse_address(host: &str) -> Option<IpAddr> {
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V6(v6)) => Some(carried_ipv4(v6).map_or(IpAddr::V6(v6), IpAddr::V4)),
        Ok(v4) => Some(v4),
        Err(_) => parse_ipv4(host).map(IpAddr::V4),
    }
}

/// The IPv4 address an IPv6 one carries: an IPv4-mapped address, or one in the NAT64 prefix
/// `64:ff9b::/96`, which a resolver on a NAT64 network answers with for an IPv4-only name.
///
/// Not the deprecated IPv4-compatible form `::a.b.c.d`, which would read `::1` as `0.0.0.1`.
///
/// `ipv4_form` in `bravebot_sandbox::address` computes the same form; neither crate may depend on
/// the other, so a change here is made there too.
fn carried_ipv4(address: Ipv6Addr) -> Option<Ipv4Addr> {
    if let Some(v4) = address.to_ipv4_mapped() {
        return Some(v4);
    }
    let [a, b, c, d] = [12, 13, 14, 15].map(|index| address.octets()[index]);
    (address.segments()[..6] == [0x64, 0xff9b, 0, 0, 0, 0]).then(|| Ipv4Addr::new(a, b, c, d))
}

/// A host in the one form two spellings of the same host share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalHost {
    /// A host written as an address, in any spelling `std` or a resolver reads, with an IPv4
    /// address carried inside an IPv6 one read as the IPv4 address.
    Address(IpAddr),
    /// A host written as a name, lowercased and without the root's trailing dot.
    Name(String),
}

/// The one form of a host as a URL, a rule or a server wrote it.
///
/// A bracketed IPv6 literal and a trailing root dot are both spellings of the same host, so one
/// pair of brackets and one trailing dot are dropped. An address is read by value, as
/// [`CanonicalHost::Address`] says. Anything else is a name, compared without regard to ASCII
/// case. A second trailing dot is kept, because a resolver does not read `example.com..` as
/// `example.com`.
pub fn canonical_host(host: &str) -> CanonicalHost {
    let host = host.strip_suffix('.').unwrap_or(host);
    let host = host
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(host);
    let host = host.to_ascii_lowercase();
    match parse_address(&host) {
        Some(address) => CanonicalHost::Address(address),
        None => CanonicalHost::Name(host),
    }
}

/// The authority a host is, where reaching it is reaching one.
///
/// Matched whole rather than searched for, because a host is a name and not a line:
/// `169.254.169.254.example.test` is somebody's domain and resolves wherever they say, so a
/// substring test here would name the metadata service for a request that never goes near it.
/// That is the one place in this module where a generous match costs something, since the
/// sentence it draws would be false rather than merely unnecessary.
pub fn at_host(host: &str) -> Option<Spent> {
    // The authority is a property of the host rather than of how a URL wrote it, so both sides
    // are put in one form first. An address is compared by value, so `2852039166`,
    // `0xA9FEA9FE` and `[::ffff:169.254.169.254]` are the metadata service as much as
    // `169.254.169.254` is; a name is compared without regard to case.
    let host = canonical_host(host);
    let named = METADATA
        .iter()
        .find(|address| canonical_host(address) == host);
    named.map(|address| Spent {
        authority: Authority::MetadataService,
        named: address,
    })
}

/// The last component of a program as a line spelled it, read as a rule reads the program word:
/// without the trailing dots, `::$DATA` suffix and `.exe`, `.com`, `.cmd` or `.bat` Windows ignores
/// ([`crate::permissions::without_suffixes`], which the wrapper table is read through).
///
/// A path and a bare name are the same claim about which authority a program reaches, so
/// `/usr/bin/docker`, `./docker` and `docker` are one answer.
fn program_name(spelled: &str) -> &str {
    let named = spelled.rsplit(['/', '\\']).next().unwrap_or(spelled);
    crate::permissions::without_suffixes(named).unwrap_or(named)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{Route, Step, Steps};
    use std::path::PathBuf;

    #[test]
    fn every_authority_is_read_back_from_its_name_and_no_other_name_is() {
        for authority in [
            Authority::ContainerDaemon,
            Authority::LoggedInTool,
            Authority::AgentSocket,
            Authority::MetadataService,
        ] {
            assert_eq!(Authority::from_name(authority.name()), Some(authority));
        }
        assert_eq!(Authority::from_name("docker"), None);
    }

    fn plan(steps: Vec<Step>) -> Plan {
        Plan {
            line: String::new(),
            directory: PathBuf::from("/work"),
            steps: Steps::Pipeline(steps),
            writes: Vec::new(),
            reads: Vec::new(),
            stdin: None,
        }
    }

    fn step(program: &str, args: &[&str]) -> Step {
        Step {
            program: program.to_string(),
            resolved: PathBuf::from(format!("/usr/bin/{}", program_name(program))),
            started_as: PathBuf::from(format!("/usr/bin/{}", program_name(program))),
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
            matched: Vec::new(),
            environment: Vec::new(),
            expansions: Vec::new(),
            routes: Vec::new(),
        }
    }

    /// The grant has to say what is being granted, and the daemon's socket is the clearest case
    /// of a capability that is reached rather than handed over.
    #[test]
    fn a_line_that_reaches_a_container_daemon_names_it() {
        let spent = spent_by(&plan(vec![step("docker", &["ps"])]));
        assert_eq!(
            spent,
            vec![Spent {
                authority: Authority::ContainerDaemon,
                named: "docker",
            }]
        );
    }

    /// Most lines reach nothing of the sort, and a prompt that said otherwise would be a sentence
    /// a reader learns to skip. The blanket line about not being sandboxed is said either way and
    /// is not this.
    #[test]
    fn an_ordinary_line_names_nothing() {
        assert!(spent_by(&plan(vec![step("ls", &["-l"])])).is_empty());
        assert!(spent_by(&plan(vec![step("cargo", &["build"])])).is_empty());
    }

    /// `$PATH` decides what a bare name means, so a tool can be reached under a name the line did
    /// not write. Either spelling naming an entry is enough, which is the opposite of what a
    /// proof about a program's behaviour may assume.
    #[test]
    fn a_tool_is_named_from_the_file_it_resolved_to_as_well_as_from_the_name() {
        let mut wrapped = step("cloud", &["s3", "ls"]);
        wrapped.resolved = PathBuf::from("/opt/homebrew/bin/aws");
        let spent = spent_by(&plan(vec![wrapped]));
        assert_eq!(
            spent,
            vec![Spent {
                authority: Authority::LoggedInTool,
                named: "aws",
            }]
        );
    }

    /// A tool that spends a credential on a few of its commands and nothing on the rest is named
    /// for those commands alone. Naming it always and naming it never are both wrong, and the two
    /// arms here are what tell those apart.
    #[test]
    fn a_tool_that_spends_on_one_command_is_named_for_that_command_alone() {
        assert!(spent_by(&plan(vec![step("git", &["log", "--oneline"])])).is_empty());
        assert_eq!(
            spent_by(&plan(vec![step("git", &["push", "origin", "main"])])),
            vec![Spent {
                authority: Authority::LoggedInTool,
                named: "git",
            }]
        );
    }

    fn names(program: &str, args: &[&str]) -> Vec<Spent> {
        spent_by(&plan(vec![step(program, args)]))
    }

    /// A program run by a wrapper reaches what it reaches without one, so `timeout 30 docker ps`
    /// is granted as `docker ps` is. The wrapper is read from the table PERM-20 reads a deny rule
    /// through, so a nested one is read the same way.
    #[test]
    fn a_program_under_a_wrapper_is_named_as_it_is_without_one() {
        let daemon = names("docker", &["ps"]);
        assert_eq!(
            daemon,
            vec![Spent {
                authority: Authority::ContainerDaemon,
                named: "docker",
            }]
        );
        for (wrapper, args) in [
            ("timeout", &["30", "docker", "ps"][..]),
            ("timeout", &["--signal", "KILL", "30", "docker", "ps"]),
            ("env", &["docker", "ps"]),
            ("env", &["-i", "A=1", "docker", "ps"]),
            ("nice", &["docker", "ps"]),
            ("nice", &["-n", "5", "docker", "ps"]),
            ("time", &["docker", "ps"]),
            ("nohup", &["docker", "ps"]),
            ("stdbuf", &["-o", "L", "docker", "ps"]),
            ("command", &["docker", "ps"]),
            ("timeout", &["5", "nice", "env", "docker", "ps"]),
        ] {
            assert_eq!(names(wrapper, args), daemon, "{wrapper} {args:?}");
        }
        assert_eq!(
            names("nice", &["gh", "api", "user"]),
            vec![Spent {
                authority: Authority::LoggedInTool,
                named: "gh",
            }]
        );
    }

    /// The wrapper's own words are not the program's arguments. `git` is named for `push` only,
    /// so the words it is given decide it, and `-f push` is the value of `time`'s `-f` option.
    #[test]
    fn a_program_under_a_wrapper_is_judged_by_its_own_arguments() {
        let git = vec![Spent {
            authority: Authority::LoggedInTool,
            named: "git",
        }];
        assert_eq!(names("timeout", &["5", "git", "push"]), git);
        assert_eq!(names("nice", &["-n", "5", "git", "fetch"]), git);
        assert!(names("timeout", &["5", "git", "log"]).is_empty());
        assert!(names("time", &["-f", "push", "git", "log"]).is_empty());
        assert!(names("timeout", &["30", "ls"]).is_empty());
    }

    /// The wrapper is spelled as PERM-20 reads it: in any case and with `.exe` or `.com` (the other
    /// spellings are in the test below).
    #[test]
    fn a_wrapper_is_read_in_any_case_and_with_an_extension() {
        let daemon = names("docker", &["ps"]);
        assert_eq!(names("TIMEOUT", &["30", "docker", "ps"]), daemon);
        assert_eq!(names("timeout.exe", &["30", "docker", "ps"]), daemon);
        assert_eq!(names("Env.COM", &["docker", "ps"]), daemon);
        // The program under the wrapper, and a program with no wrapper, fold `.com` as well.
        assert_eq!(names("timeout", &["30", "docker.com", "ps"]), daemon);
        assert_eq!(names("timeout", &["30", "DOCKER.EXE", "ps"]), daemon);
        assert_eq!(names("docker.com", &["ps"]), daemon);
    }

    /// The program word is read as the permission rules read it (PERM-5): without trailing dots,
    /// a `::$DATA` suffix, and `.cmd` or `.bat` as well as `.exe` and `.com`. One fold serves both,
    /// so a spelling the rules cover is a spelling that names the authority too (CRED-5).
    #[test]
    fn a_program_and_a_wrapper_are_read_as_the_permission_rules_read_them() {
        let daemon = names("docker", &["ps"]);
        assert_ne!(daemon, names("ls", &[]));
        for program in [
            "docker.",
            "docker.exe.",
            "docker::$DATA",
            "docker.exe::$DATA",
            "DOCKER.CMD",
            "docker.bat",
            "C:\\bin\\docker.bat",
        ] {
            assert_eq!(names(program, &["ps"]), daemon, "{program}");
            assert_eq!(
                names("timeout", &["30", program, "ps"]),
                daemon,
                "{program}"
            );
        }
        for wrapper in [
            "timeout.",
            "timeout.exe.",
            "timeout::$DATA",
            "TIMEOUT.CMD",
            "timeout.bat",
        ] {
            assert_eq!(names(wrapper, &["30", "docker", "ps"]), daemon, "{wrapper}");
        }
    }

    /// The program is read as a deny rule reads it: a letter a volume folds to an ASCII one names
    /// the same program (PERM-5), so `\u{17f}sh` is `ssh` and `\u{131}` does not hide a wrapper.
    #[test]
    fn a_program_spelled_with_a_folded_letter_is_the_program() {
        let ssh = names("ssh", &["host"]);
        assert!(!ssh.is_empty());
        assert_eq!(names("\u{17f}sh", &["host"]), ssh);
        assert_eq!(names("\u{17f}SH.exe", &["host"]), ssh);
        assert_eq!(names("t\u{131}meout", &["30", "\u{17f}sh", "host"]), ssh);
        assert!(names("\u{17f}\u{17f}sh", &["host"]).is_empty());
    }

    /// The metadata service is reached by address rather than by a program, so the address is
    /// what names it. What goes back is the address and not the argument holding it: a record
    /// holds no content, and the path of that URL is content.
    #[test]
    fn the_metadata_service_is_named_by_the_address_and_not_by_the_argument() {
        // The service answers over plaintext and nothing else, which is a fact about the
        // arrangement and half of why the clause exists: anything that can open the socket is
        // handed the role's credentials. The fixture is a string in a test and no request is
        // made.
        let spent = spent_by(&plan(vec![step(
            "curl",
            &[
                // nosemgrep: trailofbits.generic.curl-unencrypted-url.curl-unencrypted-url
                "http://169.254.169.254/latest/meta-data/iam/security-credentials/",
            ],
        )]));
        assert_eq!(
            spent,
            vec![Spent {
                authority: Authority::MetadataService,
                named: "169.254.169.254",
            }]
        );
        assert!(
            !spent[0].named.contains("security-credentials"),
            "the record kept the argument rather than the address in it"
        );
    }

    /// A daemon reached with a socket and a generic client runs none of the programs in the
    /// table, and it is the same authority.
    #[test]
    fn a_socket_named_in_an_argument_is_the_daemon_it_belongs_to() {
        // The daemon's API is spoken over the socket rather than over the network, so the URL
        // carries a scheme and reaches nothing: the host in it is the socket. A string in a
        // test, and no request is made.
        let spent = spent_by(&plan(vec![step(
            "curl",
            &[
                "--unix-socket",
                "/var/run/docker.sock",
                // nosemgrep: trailofbits.generic.curl-unencrypted-url.curl-unencrypted-url
                "http://v1/containers/json",
            ],
        )]));
        assert_eq!(
            spent,
            vec![Spent {
                authority: Authority::ContainerDaemon,
                named: "docker.sock",
            }]
        );
    }

    /// An assignment in front of a program decides what that program reaches, which is the whole
    /// reason for writing one, so it is read the same way an argument is.
    #[test]
    fn an_assignment_in_front_of_a_program_names_what_it_reaches() {
        let mut forwarded = step("make", &["deploy"]);
        forwarded.environment = vec![("SSH_AUTH_SOCK".to_string(), "/tmp/agent.7".to_string())];
        let spent = spent_by(&plan(vec![forwarded]));
        assert_eq!(
            spent,
            vec![Spent {
                authority: Authority::AgentSocket,
                named: "SSH_AUTH_SOCK",
            }]
        );
    }

    /// An assignment decides what a program reaches; an expansion is read the same way, by its
    /// name and by the text and variable it is written with (CMDLINE-17).
    #[test]
    fn an_expansion_in_front_of_a_program_names_what_it_reaches() {
        let mut forwarded = step("make", &["deploy"]);
        forwarded.expansions = vec![crate::command::Expansion {
            name: "SSH_AUTH_SOCK".to_string(),
            parts: vec![crate::command::ValuePart::Variable("AGENT".to_string())],
        }];
        assert_eq!(
            spent_by(&plan(vec![forwarded])),
            vec![Spent {
                authority: Authority::AgentSocket,
                named: "SSH_AUTH_SOCK",
            }]
        );
        assert!(spent_by(&plan(vec![step("make", &["deploy"])])).is_empty());
    }

    /// Two tools in one line are two grants, because approving it grants both.
    #[test]
    fn each_authority_a_line_reaches_is_named_once() {
        let spent = spent_by(&plan(vec![
            step("gh", &["pr", "list"]),
            step("docker", &["build", "."]),
            step("gh", &["pr", "view"]),
        ]));
        assert_eq!(
            spent,
            vec![
                Spent {
                    authority: Authority::LoggedInTool,
                    named: "gh",
                },
                Spent {
                    authority: Authority::ContainerDaemon,
                    named: "docker",
                },
            ]
        );
    }

    /// A route is a destination rather than an authority, so a line that writes one names nothing
    /// on this account.
    #[test]
    fn a_redirection_names_no_authority() {
        let mut writing = step("cat", &["notes.txt"]);
        writing.routes = vec![Route::Stdout {
            path: PathBuf::from("/work/out.txt"),
            append: false,
        }];
        assert!(spent_by(&plan(vec![writing])).is_empty());
    }

    /// A host grant reaches the same service without any line at all, so the question is asked of
    /// a host too.
    #[test]
    fn a_request_to_the_metadata_service_is_named_by_its_host() {
        assert_eq!(
            at_host("169.254.169.254"),
            Some(Spent {
                authority: Authority::MetadataService,
                named: "169.254.169.254",
            })
        );
        assert_eq!(
            at_host("Metadata.Google.Internal."),
            Some(Spent {
                authority: Authority::MetadataService,
                named: "metadata.google.internal",
            })
        );
        assert_eq!(at_host("example.test"), None);
    }

    /// A host is matched whole. Anybody may register a name with the address inside it and point
    /// it wherever they like, and a prompt saying such a request reaches this machine's own
    /// metadata service would be telling the person something untrue.
    #[test]
    fn a_host_that_merely_contains_the_address_is_not_the_metadata_service() {
        assert_eq!(at_host("169.254.169.254.example.test"), None);
        assert_eq!(at_host("not-metadata.google.internal"), None);
    }

    /// Name resolution reads a number as an address however it is spelled, and every spelling of
    /// the metadata service's address is the metadata service: a prompt naming it only when the
    /// spelling is the usual one would be silent about the request that goes where the usual one
    /// does.
    #[test]
    fn a_numeric_spelling_of_the_metadata_address_is_the_metadata_service() {
        for spelling in [
            "169.254.169.254",
            "169.254.169.254.",
            "2852039166",
            "0xA9FEA9FE",
            "0xA9.0xFE.0xA9.0xFE",
            "0251.0376.0251.0376",
            "169.254.43518",
        ] {
            assert_eq!(
                at_host(spelling),
                Some(Spent {
                    authority: Authority::MetadataService,
                    named: "169.254.169.254",
                }),
                "{spelling}"
            );
        }
    }

    /// A number one past the address, or an address on the same subnet, is somebody else's host,
    /// and naming the metadata service for it would be as untrue as missing the service itself.
    #[test]
    fn a_numeric_spelling_of_another_address_is_not_the_metadata_service() {
        assert_eq!(at_host("169.254.169.255"), None);
        assert_eq!(at_host("2852039167"), None);
        assert_eq!(at_host("evil.example"), None);
    }

    /// A connection to an IPv4 address written inside an IPv6 one goes to the IPv4 address, which
    /// is how FETCH-8 and SANDBOX-24 classify it. So `[::ffff:169.254.169.254]` and the NAT64
    /// spelling `64:ff9b::a9fe:a9fe` are the metadata service, for every IPv4 address it answers
    /// on.
    #[test]
    fn an_ipv4_address_inside_ipv6_is_the_metadata_service_it_carries() {
        for (spelling, named) in [
            ("[::ffff:169.254.169.254]", "169.254.169.254"),
            ("::ffff:169.254.169.254", "169.254.169.254"),
            ("::ffff:a9fe:a9fe", "169.254.169.254"),
            ("[::FFFF:A9FE:A9FE]", "169.254.169.254"),
            ("[::ffff:169.254.169.254].", "169.254.169.254"),
            ("64:ff9b::169.254.169.254", "169.254.169.254"),
            ("[64:ff9b::a9fe:a9fe]", "169.254.169.254"),
            ("[::ffff:169.254.170.2]", "169.254.170.2"),
            ("64:ff9b::a9fe:aa02", "169.254.170.2"),
            ("[::ffff:100.100.100.200]", "100.100.100.200"),
            ("64:ff9b::6464:64c8", "100.100.100.200"),
            ("[fd00:ec2::254]", "fd00:ec2::254"),
        ] {
            assert_eq!(
                at_host(spelling),
                Some(Spent {
                    authority: Authority::MetadataService,
                    named,
                }),
                "{spelling}"
            );
        }
    }

    /// Only the two prefixes that carry an IPv4 address are read through. An IPv6 address that
    /// happens to end in the metadata address's 32 bits is another host, the deprecated
    /// IPv4-compatible `::a9fe:a9fe` among them, and so is a mapped address one past it.
    #[test]
    fn an_ipv6_address_carrying_another_address_is_not_the_metadata_service() {
        for spelling in [
            "[::ffff:169.254.169.255]",
            "64:ff9b::a9fe:a9ff",
            "[fe80::a9fe:a9fe]",
            "2001:db8::a9fe:a9fe",
            "64:ff9b:0:0:1::a9fe:a9fe",
            "[::a9fe:a9fe]",
        ] {
            assert_eq!(at_host(spelling), None, "{spelling}");
        }
    }

    /// One form for a host, so two spellings of one host compare equal and a name stays a name.
    #[test]
    fn a_host_is_put_in_one_form_whatever_its_spelling() {
        let metadata = CanonicalHost::Address(IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254)));
        for spelling in [
            "169.254.169.254",
            "169.254.169.254.",
            "[::ffff:169.254.169.254]",
            "::FFFF:a9fe:a9fe",
            "64:ff9b::a9fe:a9fe",
            "2852039166",
        ] {
            assert_eq!(canonical_host(spelling), metadata, "{spelling}");
        }
        assert_eq!(
            canonical_host("[::1]"),
            CanonicalHost::Address("::1".parse().expect("an address"))
        );
        assert_eq!(
            canonical_host("Docs.Example.COM."),
            CanonicalHost::Name("docs.example.com".to_string())
        );
        // One trailing dot is the root; a second one is part of what was written.
        assert_eq!(
            canonical_host("example.com.."),
            CanonicalHost::Name("example.com.".to_string())
        );
    }

    /// A sign is not part of a number to a resolver, so a spelling holding one is not an address.
    #[test]
    fn a_signed_part_is_not_a_spelling_of_the_metadata_address() {
        assert_eq!(at_host("+169.254.169.254"), None);
        assert_eq!(at_host("169.254.169.+254"), None);
        assert_eq!(at_host("0x+A9FEA9FE"), None);
    }
}
