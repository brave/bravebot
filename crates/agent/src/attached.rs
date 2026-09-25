//! Files dropped onto a line whose argument is sent, read before the request that carries them.
//!
//! A turn reads its own attachments inside the policy it precommitted, because a turn has one that
//! holds [`Capability::FileRead`]. The two requests that are not turns do not: [`crate::turn::aside`]
//! grants [`Capability::WebFetch`] alone and takes no workspace, and [`crate::manifest`]'s planner is
//! the network and nothing else, because a planner cannot read and cannot write. Neither of those is
//! weakened here. The read happens before either planner's policy exists, under a policy of its own
//! holding `FileRead` and nothing else, with the dropped paths precommitted into its routing from the
//! gesture that produced them.
//!
//! That keeps both properties worth keeping. The read is gated and named in the trail, exactly as a
//! file carried into a turn is; and the planner still cannot reach a file itself, because what it is
//! handed is bytes something else already read and labelled.
//!
//! Only a picture or a PDF comes through here. A dropped text file's contents would be a context
//! message, and both of these planners are precommitted to a context holding the task and the
//! driver's own words and nothing read out of the tree, so what a text file becomes on one of these
//! lines is its name (`dropping.md` DROP-4). The caller settles that in the argument before it gets
//! here, which is why nothing in this module has a text case.

use bravebot_aichat::protocol::{ImageUrl, Part};
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::Sink;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_core::reference::Presentation;
use bravebot_core::slot::{SlotId, SlotStore};
use bravebot_core::trust::TrustStore;
use bravebot_core::value::Labelled;

use crate::turn::{Attachment, TurnError};
use crate::workspace::Workspace;

/// One dropped file, read and settled before the request that carries it exists.
///
/// Settled here rather than at the request, because the label belongs to the read: the policy that
/// did it is the one that knows what the trust map said, and it is gone by the time the request is
/// assembled. What travels is the kernel's answer rather than the bytes and a question about them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Carried {
    /// The bytes, as the data URI a request holds a picture in.
    Shown {
        /// The path the marker stood for, as the workspace names it.
        path: String,
        /// `data:<media>;base64,...`, which is what the kernel released.
        uri: String,
    },
    /// What the planner is told instead, for a file the kernel would not show it.
    ///
    /// Not reached by a file somebody dropped, because dropping it is the grant and the grant is
    /// recorded before the read: `dropping.md` DROP-2 makes the file trusted whatever covers the
    /// directory around it. Here because the alternative is handing over bytes the label says the
    /// planner may not have, which is the one thing this must never do.
    Described {
        /// The path, which travels either way: a planner told nothing about what it cannot see is
        /// a planner that read a marker standing for nothing.
        path: String,
        /// The kernel's own description of what it withheld.
        said: String,
    },
}

impl Carried {
    /// What the request holds for it, in the message the words it was dropped beside are in.
    pub fn part(&self) -> Part {
        match self {
            Self::Shown { uri, .. } => Part::ImageUrl {
                image_url: ImageUrl { url: uri.clone() },
            },
            Self::Described { path, said } => Part::Text {
                text: format!("{path} could not be shown to you.\n\n{said}"),
            },
        }
    }
}

/// Read what a person dropped onto the line, before the planner that will be sent it exists.
///
/// `dropped` is what the markers in the line stood for, in the order they number them, so a planner
/// reading `[Image #2]` can count to the picture that answers it.
///
/// `trust` is the caller's own map, and it is written back to as each drop is read rather than
/// copied and left behind. Dropping a file records a rule for that file, and the rule is the
/// person's rather than this request's: `dropping.md` DROP-2 has it hold for the rest of the
/// session, so the same picture is shown again without being dragged again. Written back on the way
/// out too, since the gesture is what granted the rule and a sibling file that could not be read
/// says nothing about it.
///
/// Nothing at all for a line that dropped nothing, which is the ordinary case: a policy needs
/// routing to precommit and there is nothing here to put in it.
pub fn read<S: Sink>(
    workspace: &Workspace,
    dropped: &[Attachment],
    trust: &mut TrustStore,
    sink: &mut S,
) -> Result<Vec<Carried>, TurnError> {
    if dropped.is_empty() {
        return Ok(Vec::new());
    }

    // Fixed before anything is read, and fixed from the gesture: the person dragged that file onto
    // the window and let it go, so the path is settled before any request goes out. That is the same
    // footing a dropped file has in a turn, and the reason `dropping.md` DROP-1 can live at the call
    // site rather than in the bytes.
    let mut routing = Routing::new();
    for (index, file) in dropped.iter().enumerate() {
        routing.insert_trusted(format!("dropped_{index}"), file.path.clone());
    }

    let mut policy = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead]),
        sink,
    )
    .map_err(|d| TurnError::Precommit(d.to_string()))?
    .with_trust(trust.clone())
    .with_root(workspace.root())
    .with_scratch(workspace.scratch())
    .with_backslash_separates(crate::workspace::BACKSLASH_SEPARATES);

    // The quarantine of a request nothing resumes. A conversation keeps one so a later round can
    // resolve a reference; there is no later round here and no tool to resolve one with, so what a
    // withheld file leaves behind is the sentence describing it and nothing a planner could ask for.
    let mut slots = SlotStore::new();
    let mut carried = Vec::new();

    for (index, file) in dropped.iter().enumerate() {
        // Out of routing rather than off `file`, so the path that is read is the one the precommit
        // fixed: a second reading of the same field is a second answer waiting to disagree with it.
        // nosemgrep: trailofbits.rs.panic-in-function-returning-result.panic-in-function-returning-result
        let path = policy
            .routing()
            .get(&format!("dropped_{index}"))
            .expect("routing was precommitted with this key")
            .to_string();

        // Dropping the file is the grant, exactly as naming one with `@` is. Recorded before the
        // read so the read sees it, and under the name the read will ask about.
        policy.vouch_for_named_path(&workspace.trust_key(&path));

        let contents = match workspace.read_dropped_attachment(
            &mut policy,
            &Labelled::trusted(path.clone()),
            &file.media,
        ) {
            Ok(contents) => contents,
            // Taken on the way out as well as at the end, because the rules already recorded are
            // for files this person dropped and a file that would not open is not a reason to take
            // them back.
            Err(error) => {
                *trust = policy.trust();
                policy.finish();
                return Err(error.into());
            }
        };

        // The kernel decides, from the label alone, whether the planner may see the bytes. Nothing
        // here can ask for them, which is the point.
        let presented = match policy.present(
            "chat",
            SlotId::new(format!("ref:{index}")),
            &path,
            &contents,
            &mut slots,
        ) {
            Ok(presented) => presented,
            Err(denial) => {
                *trust = policy.trust();
                policy.finish();
                return Err(TurnError::Precommit(denial.to_string()));
            }
        };

        carried.push(match presented {
            Presentation::Visible(uri) => Carried::Shown { path, uri },
            Presentation::Quarantined(reference) => Carried::Described {
                path,
                said: reference.of_a_picture(&file.media).describe(),
            },
        });
    }

    // Taken before `finish` consumes the policy, the way a turn takes its own, because the rules
    // each drop recorded are in here and nowhere else.
    *trust = policy.trust();
    policy.finish();
    Ok(carried)
}
