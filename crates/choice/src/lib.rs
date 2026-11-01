//! Which whole number a processor's answer chose.
//!
//! A processor asked to choose one entry of a numbered list answers in text. This crate decides
//! from that text which number it chose, from 1 to a limit the planner wrote, or 0 where it chose
//! none. It is not the driver: the text reaches it only through the policy layer's gate for this
//! one read, [`Policy::read_a_number`], and what comes back out of the answer is one byte.
//!
//! The crate holds no `declassify` and no `Labelled::new`. A second comparison of an answer's text
//! anywhere in it is found by reading the diff, and a second reader of an answer anywhere is a
//! moved count in `docs/specs/labels.md`.
#![forbid(unsafe_code)]

mod number;

pub use number::number_in;

use bravebot_core::{Denial, Labelled, Policy, ProcessorSpec, Sink};

/// Read a processor's answer for the number it chose, labelled as the answer was.
///
/// The one call of [`Policy::read_a_number`] outside the policy layer. The byte is 0 where the
/// processor chose none, and a spec that was not built to choose is refused.
pub fn read<S: Sink>(
    policy: &mut Policy<'_, S>,
    spec: &ProcessorSpec,
    answer: Labelled<String>,
) -> Result<Labelled<u8>, Denial> {
    policy.read_a_number(spec, answer, number_in)
}

#[cfg(test)]
mod tests {
    use super::read;
    use bravebot_core::{
        CapabilitySet, Label, Policy, RecordingSink, ReleasePlan, Routing, SlotId, SlotStore,
    };

    /// A slot written at `label`, standing for a value a test would otherwise have to build.
    fn slot_of(slots: &mut SlotStore, name: &str, label: Label, text: &str) -> SlotId {
        let id = SlotId::new(name);
        slots
            .writer_for(id.clone(), label)
            .expect("a new slot")
            .write(text)
            .expect("a first write");
        id
    }

    /// The gate is called with `number_in` and the spec's own limit, and the byte is labelled as
    /// the answer was. A `read` that passed another reader, or the largest limit, gives 6 for a
    /// list of five, and one that labelled the byte itself would not follow the answer's label.
    #[test]
    fn reading_an_answer_applies_the_planners_limit_and_labels_the_byte_as_the_answer_was() {
        let mut slots = SlotStore::new();
        let page = slot_of(&mut slots, "ref:page", Label::trusted_public(), "a page");
        let instruction = slot_of(&mut slots, "ref:said", Label::trusted_public(), "choose");
        let instruction = slots.take_for_reading(&instruction).expect("written");

        let mut sink = RecordingSink::new();
        let mut routing = Routing::new();
        routing.insert_trusted("task", "choose");
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::none(),
            &mut sink,
        )
        .expect("a policy");
        let spec = policy
            .before_chooser("p", &[page], &instruction, 5, &slots)
            .expect("a processor asked to choose");

        for (n, (answer, expected)) in [(" 5\n", 5), ("6", 0), ("7 and 8", 0), ("", 0)]
            .into_iter()
            .enumerate()
        {
            let reply = slot_of(
                &mut slots,
                &format!("ref:{n}"),
                Label::trusted_public(),
                answer,
            );
            let reply = slots.take_for_reading(&reply).expect("written");
            let number = read(&mut policy, &spec, reply).expect("a processor that chooses");
            assert_eq!(number.label(), Label::trusted_public());
            assert_eq!(number.into_trusted().expect("trusted and public"), expected);
        }

        let reply = slot_of(&mut slots, "ref:untrusted", Label::untrusted_public(), "3");
        let reply = slots.take_for_reading(&reply).expect("written");
        let number = read(&mut policy, &spec, reply).expect("a processor that chooses");
        assert_eq!(number.label(), Label::untrusted_public());
        assert!(
            number.into_trusted().is_err(),
            "a number read from untrusted text was readable"
        );
    }

    /// A spec built for a document is refused by the gate, so this crate cannot be used to read a
    /// number out of a document processor's answer.
    #[test]
    fn a_processor_that_was_not_asked_to_choose_has_no_number_read_from_its_answer() {
        let mut slots = SlotStore::new();
        let page = slot_of(&mut slots, "ref:page", Label::trusted_public(), "a page");
        let instruction = slot_of(&mut slots, "ref:said", Label::trusted_public(), "rewrite");
        let instruction = slots.take_for_reading(&instruction).expect("written");

        let mut sink = RecordingSink::new();
        let mut routing = Routing::new();
        routing.insert_trusted("task", "rewrite");
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::none(),
            &mut sink,
        )
        .expect("a policy");
        let spec = policy
            .before_processor("p", &[page], &instruction, None, &slots)
            .expect("a processor that writes a document");

        let reply = slot_of(&mut slots, "ref:reply", Label::trusted_public(), "7");
        let reply = slots.take_for_reading(&reply).expect("written");
        read(&mut policy, &spec, reply)
            .expect_err("a document processor's answer was read for a number");
    }
}
