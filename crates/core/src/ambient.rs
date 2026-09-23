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
        let crate::command::Step {
            program,
            resolved,
            args,
            environment,
            routes: _,
        } = step;
        for spelling in [program.as_str(), &resolved.to_string_lossy()] {
            let name = program_name(spelling);
            for entry in REACHING {
                let called = entry.program.eq_ignore_ascii_case(name);
                let spending = entry.spends.is_empty()
                    || args.iter().any(|arg| entry.spends.contains(&arg.as_str()));
                if called && spending {
                    found(entry.authority, entry.program);
                }
            }
        }
        // Every other string the step carries. An assignment written in front of a program is
        // read as well as its arguments: `DOCKER_HOST` and `SSH_AUTH_SOCK` are said there rather
        // than in an argument, and the point of an assignment is that it decides what the program
        // reaches.
        let written = args.iter().map(String::as_str).chain(
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

/// The authority a host is, where reaching it is reaching one.
///
/// Matched whole rather than searched for, because a host is a name and not a line:
/// `169.254.169.254.example.test` is somebody's domain and resolves wherever they say, so a
/// substring test here would name the metadata service for a request that never goes near it.
/// That is the one place in this module where a generous match costs something, since the
/// sentence it draws would be false rather than merely unnecessary.
pub fn at_host(host: &str) -> Option<Spent> {
    // A bracketed IPv6 literal and a trailing root dot are both spellings of the same host, and
    // the authority is a property of the host rather than of how a URL wrote it.
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.');
    METADATA
        .iter()
        .find(|address| address.eq_ignore_ascii_case(host))
        .map(|address| Spent {
            authority: Authority::MetadataService,
            named: address,
        })
}

/// The last component of a program as a line spelled it, without the extension Windows writes.
///
/// A path and a bare name are the same claim about which authority a program reaches, so
/// `/usr/bin/docker`, `./docker` and `docker` are one answer.
fn program_name(spelled: &str) -> &str {
    let named = spelled.rsplit(['/', '\\']).next().unwrap_or(spelled);
    match named.len().checked_sub(4) {
        Some(stem) if named[stem..].eq_ignore_ascii_case(".exe") => &named[..stem],
        _ => named,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{Route, Step, Steps};
    use std::path::PathBuf;

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
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
            environment: Vec::new(),
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
}
