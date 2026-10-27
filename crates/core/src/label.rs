//! The label lattice `L = I × C`.
//!
//! ```text
//! I ∈ {T, U}          integrity:       T = trusted, U = untrusted
//! C ∈ {pub, priv}     confidentiality: pub = public, priv = private
//!
//! (T,pub) → (T,priv) → (U,priv)
//! (T,pub) → (U,pub)  → (U,priv)
//! ```
//!
//! An arrow is a degradation ([`Label::degrades_to`]): integrity may go from T to U and
//! confidentiality from pub to priv. `(T,priv)` and `(U,pub)` are **incomparable**:
//! neither degrades to the other. That is what makes this a lattice rather than a pair
//! of booleans, and it is why [`Label`] implements neither [`PartialOrd`] nor [`Ord`]:
//! either would rank the two.
//!
//! Taint moves in opposite directions on the two axes: integrity **meets** (one
//! untrusted input taints the result) while confidentiality **joins** (one private
//! input makes the result private). See [`taint_all`].

use std::fmt;

/// Integrity axis. `U` is bottom: untrusted values cannot become trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Integrity {
    /// Trusted, meaning derived only from trusted input.
    Trusted,
    /// Untrusted, meaning derived from content we do not control.
    Untrusted,
}

impl Integrity {
    /// Lattice meet (⊓). `U` is bottom, so `meet(T, U) == U`.
    pub fn meet(self, other: Self) -> Self {
        if self == Self::Untrusted || other == Self::Untrusted {
            Self::Untrusted
        } else {
            Self::Trusted
        }
    }
}

/// Confidentiality axis. `priv` is top: private values cannot become public
/// without an explicit, audited declassification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Confidentiality {
    /// Public, so safe to release.
    Public,
    /// Private, meaning confidential; must be declassified before crossing a bridge.
    Private,
}

impl Confidentiality {
    /// Lattice join (⊔). `priv` is top, so `join(pub, priv) == priv`.
    pub fn join(self, other: Self) -> Self {
        if self == Self::Private || other == Self::Private {
            Self::Private
        } else {
            Self::Public
        }
    }
}

/// A provenance label: one point in the lattice.
///
/// A label has no `PartialOrd` or `Ord`, because the two labels that neither degrades
/// into the other have no rank. Comparing two with `<` does not compile:
///
/// ```compile_fail
/// use bravebot_core::Label;
/// let _ = Label::trusted_public() < Label::untrusted_private();
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Label {
    pub integrity: Integrity,
    pub confidentiality: Confidentiality,
}

impl Label {
    pub const fn new(integrity: Integrity, confidentiality: Confidentiality) -> Self {
        Self {
            integrity,
            confidentiality,
        }
    }

    /// `(T,pub)`, the only label routing fields accept.
    pub const fn trusted_public() -> Self {
        Self::new(Integrity::Trusted, Confidentiality::Public)
    }

    pub const fn trusted_private() -> Self {
        Self::new(Integrity::Trusted, Confidentiality::Private)
    }

    pub const fn untrusted_public() -> Self {
        Self::new(Integrity::Untrusted, Confidentiality::Public)
    }

    pub const fn untrusted_private() -> Self {
        Self::new(Integrity::Untrusted, Confidentiality::Private)
    }

    /// Whether `self` may be *degraded* into `other`.
    ///
    /// Degradation is the direction taint travels: integrity may go trusted →
    /// untrusted, confidentiality may go public → private, never the reverse. A check
    /// that let integrity move toward trusted would let a relabel launder untrusted
    /// content into trusted.
    pub fn degrades_to(self, other: Self) -> bool {
        other.integrity == self.integrity.meet(other.integrity)
            && other.confidentiality == self.confidentiality.join(other.confidentiality)
    }

    /// The label whose [`fmt::Display`] form is `word`, or `None` for any other word.
    ///
    /// The way a label is read back after it crossed a boundary as a word. Nothing else is
    /// accepted: no case folding, no spaces, no label a reader would have to guess at.
    pub fn named(word: &str) -> Option<Self> {
        [
            Self::trusted_public(),
            Self::trusted_private(),
            Self::untrusted_public(),
            Self::untrusted_private(),
        ]
        .into_iter()
        .find(|label| label.to_string() == word)
    }

    pub fn is_trusted(self) -> bool {
        self.integrity == Integrity::Trusted
    }

    pub fn is_public(self) -> bool {
        self.confidentiality == Confidentiality::Public
    }
}

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let i = match self.integrity {
            Integrity::Trusted => "T",
            Integrity::Untrusted => "U",
        };
        let c = match self.confidentiality {
            Confidentiality::Public => "pub",
            Confidentiality::Private => "priv",
        };
        write!(f, "({i},{c})")
    }
}

/// Output label for a computation, given its inputs.
///
/// Integrity meets, confidentiality joins. No inputs means no taint, so the result
/// is `(T,pub)`, the identity for both operations.
pub fn taint_all(labels: impl IntoIterator<Item = Label>) -> Label {
    labels.into_iter().fold(Label::trusted_public(), |acc, l| {
        Label::new(
            acc.integrity.meet(l.integrity),
            acc.confidentiality.join(l.confidentiality),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const T_PUB: Label = Label::trusted_public();
    const T_PRIV: Label = Label::trusted_private();
    const U_PUB: Label = Label::untrusted_public();
    const U_PRIV: Label = Label::untrusted_private();

    /// A label read back from the word it was written as is the label, and any other word names
    /// none, so a boundary that carried a label as text cannot be made to read a better one.
    #[test]
    fn a_label_reads_back_from_its_word_and_from_nothing_else() {
        for label in [T_PUB, T_PRIV, U_PUB, U_PRIV] {
            assert_eq!(Label::named(&label.to_string()), Some(label));
        }
        for word in [
            "",
            "T",
            "(t,pub)",
            "(T,Pub)",
            "( T,pub)",
            "(T, pub)",
            "(T,pub) ",
            "trusted",
            "(T,pub)x",
            "(T,public)",
            "(X,pub)",
        ] {
            assert_eq!(Label::named(word), None, "{word:?}");
        }
    }

    #[test]
    fn integrity_meet_is_pessimistic() {
        use Integrity::*;
        assert_eq!(Trusted.meet(Trusted), Trusted);
        assert_eq!(Trusted.meet(Untrusted), Untrusted);
        assert_eq!(Untrusted.meet(Trusted), Untrusted);
        assert_eq!(Untrusted.meet(Untrusted), Untrusted);
    }

    #[test]
    fn confidentiality_join_is_pessimistic() {
        use Confidentiality::*;
        assert_eq!(Public.join(Public), Public);
        assert_eq!(Public.join(Private), Private);
        assert_eq!(Private.join(Public), Private);
        assert_eq!(Private.join(Private), Private);
    }

    /// `(T,pub)` degrades into every label and `(U,priv)` only into itself, so the
    /// order has one source and one sink.
    #[test]
    fn degradation_has_one_source_and_one_sink() {
        for l in [T_PUB, T_PRIV, U_PUB, U_PRIV] {
            assert!(T_PUB.degrades_to(l), "(T,pub) must degrade to {l}");
            assert_eq!(U_PRIV.degrades_to(l), l == U_PRIV, "(U,priv) to {l}");
        }
    }

    /// The property that makes this a lattice: the middle elements are incomparable,
    /// so neither degrades to the other.
    #[test]
    fn middle_elements_are_incomparable() {
        assert!(!T_PRIV.degrades_to(U_PUB));
        assert!(!U_PUB.degrades_to(T_PRIV));
    }

    #[test]
    fn degradation_is_antisymmetric() {
        for a in [T_PUB, T_PRIV, U_PUB, U_PRIV] {
            for b in [T_PUB, T_PRIV, U_PUB, U_PRIV] {
                if a.degrades_to(b) && b.degrades_to(a) {
                    assert_eq!(a, b);
                }
            }
        }
    }

    #[test]
    fn degradation_is_transitive() {
        let all = [T_PUB, T_PRIV, U_PUB, U_PRIV];
        for a in all {
            for b in all {
                for c in all {
                    if a.degrades_to(b) && b.degrades_to(c) {
                        assert!(a.degrades_to(c), "{a} → {b} → {c} but not {a} → {c}");
                    }
                }
            }
        }
    }

    /// A degradation never raises integrity and never lowers confidentiality, so a
    /// value may not be *upgraded* from untrusted to trusted or declassified from
    /// private to public by relabelling.
    #[test]
    fn degradation_never_raises_integrity_or_lowers_confidentiality() {
        for a in [T_PUB, T_PRIV, U_PUB, U_PRIV] {
            for b in [T_PUB, T_PRIV, U_PUB, U_PRIV] {
                if a.degrades_to(b) {
                    assert!(
                        a.is_trusted() || !b.is_trusted(),
                        "{a} → {b} raises integrity"
                    );
                    assert!(
                        a.is_public() || !b.is_public(),
                        "{a} → {b} lowers confidentiality"
                    );
                }
            }
        }
        assert!(!U_PUB.degrades_to(T_PUB));
        assert!(T_PUB.degrades_to(U_PUB));
    }

    #[test]
    fn degradation_is_reflexive() {
        for l in [T_PUB, T_PRIV, U_PUB, U_PRIV] {
            assert!(l.degrades_to(l));
        }
    }

    #[test]
    fn top_of_taint_degrades_from_everything() {
        for l in [T_PUB, T_PRIV, U_PUB, U_PRIV] {
            assert!(l.degrades_to(U_PRIV), "{l} must degrade to (U,priv)");
        }
        assert!(!U_PRIV.degrades_to(T_PUB));
    }

    /// Degradation agrees with taint: combining inputs always yields a label each
    /// input can degrade into.
    #[test]
    fn taint_result_is_a_degradation_of_its_inputs() {
        let all = [T_PUB, T_PRIV, U_PUB, U_PRIV];
        for a in all {
            for b in all {
                let out = taint_all([a, b]);
                assert!(a.degrades_to(out), "{a} must degrade to taint {out}");
                assert!(b.degrades_to(out), "{b} must degrade to taint {out}");
            }
        }
    }

    #[test]
    fn no_inputs_means_no_taint() {
        assert_eq!(taint_all([]), T_PUB);
    }

    #[test]
    fn one_untrusted_input_taints_the_result() {
        assert_eq!(taint_all([T_PUB, T_PUB]), T_PUB);
        assert_eq!(taint_all([T_PUB, U_PUB]), U_PUB);
    }

    #[test]
    fn one_private_input_makes_the_result_private() {
        assert_eq!(taint_all([T_PUB, T_PRIV]), T_PRIV);
    }

    /// A trusted-private input and an untrusted-public input together produce
    /// untrusted-private: each axis degrades independently.
    #[test]
    fn axes_degrade_independently() {
        assert_eq!(taint_all([T_PRIV, U_PUB]), U_PRIV);
    }

    #[test]
    fn taint_is_order_independent() {
        assert_eq!(taint_all([T_PRIV, U_PUB]), taint_all([U_PUB, T_PRIV]));
    }

    /// Taint moves down the integrity axis while moving up the confidentiality axis,
    /// so the guarantee is stated per axis: the result is never more trusted, and
    /// never less private, than any input.
    #[test]
    fn taint_only_degrades_on_each_axis() {
        let all = [T_PUB, T_PRIV, U_PUB, U_PRIV];
        for a in all {
            for b in all {
                let out = taint_all([a, b]);
                for input in [a, b] {
                    assert!(
                        input.is_trusted() || !out.is_trusted(),
                        "taint of {a} and {b} is {out}, more trusted than input {input}"
                    );
                    assert!(
                        input.is_public() || !out.is_public(),
                        "taint of {a} and {b} is {out}, less private than input {input}"
                    );
                }
            }
        }
    }

    /// A trusted input cannot launder an untrusted one by being combined with it.
    #[test]
    fn trusted_input_cannot_launder_untrusted() {
        for trusted in [T_PUB, T_PRIV] {
            for untrusted in [U_PUB, U_PRIV] {
                assert!(!taint_all([trusted, untrusted]).is_trusted());
            }
        }
    }

    #[test]
    fn display_matches_the_canonical_notation() {
        assert_eq!(T_PUB.to_string(), "(T,pub)");
        assert_eq!(U_PRIV.to_string(), "(U,priv)");
    }
}
