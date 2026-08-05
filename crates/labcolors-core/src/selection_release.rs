//! The sole authored selection release over the hard-feasible set (V5c-1).
//!
//! One versioned [`SelectionReleaseV1`] is the only authored selection input:
//! it declares a total preorder over opaque canonical candidate keys as an
//! ordered sequence of tie groups, and the single common tie-break inside a
//! group is the canonical key bytes themselves. Declaration index, `usize`
//! positions, RGB bytes, distances and weights never participate in the
//! order. The module admits the release once, seals it, and content-addresses
//! it; materialisation sorts exclusively by the admitted rank and the
//! canonical key, so no evaluator ever ranks or selects.

use std::collections::BTreeMap;

use crate::program_session::{DeclaredJointSelectionV1, JointCandidateStateV1};
use crate::sha256;

const IDENTITY_DOMAIN_V1: &[u8] = b"labcolors.selection-release.v1\0";

/// Typed admission and materialisation failures of the selection release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionReleaseErrorV1 {
    /// The release declares no tie groups, so no preorder exists.
    EmptyRelease,
    /// A tie group carries no candidate keys.
    EmptyRankGroup,
    /// A candidate key has no bytes and cannot identify a candidate.
    EmptyCandidateKey,
    /// One canonical key appears in more than one place of the release.
    DuplicateCandidateKey,
    /// A candidate binds a key that the release does not rank.
    UnknownCandidateKey,
    /// Two candidates bind the same canonical key.
    DuplicateCandidateBinding,
    /// Selection was asked to order an empty candidate set.
    EmptyCandidateSet,
    /// A release length field cannot be encoded, so the identity grammar is
    /// unrepresentable.
    ReleaseShapeOverflow,
}

/// One opaque canonical candidate key.
///
/// The key is authored identity: its bytes are the only property the release
/// may ever inspect. No RGB bytes, declaration index, distance or weight
/// semantics are read from it anywhere in this module.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SelectionCandidateKeyV1(Box<[u8]>);

impl SelectionCandidateKeyV1 {
    pub(crate) fn new(bytes: Box<[u8]>) -> Self {
        Self(bytes)
    }

    fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// The authored release shape before admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectionReleaseV1 {
    revision: u64,
    rank_groups: Box<[Box<[SelectionCandidateKeyV1]>]>,
}

impl SelectionReleaseV1 {
    pub(crate) fn new(revision: u64, rank_groups: Box<[Box<[SelectionCandidateKeyV1]>]>) -> Self {
        Self {
            revision,
            rank_groups,
        }
    }
}

/// A sealed selection release: the total preorder is admitted, canonical and
/// content-addressed.
#[derive(Debug, Clone)]
pub(crate) struct AdmittedSelectionReleaseV1 {
    revision: u64,
    identity: [u8; 32],
    ranks: BTreeMap<Vec<u8>, usize>,
}

/// One u32 length field of the identity grammar.
///
/// The encoding is fail-closed: a length that does not fit u32 is a typed
/// rejection, never a panic.
fn length_field_v1(value: usize) -> Result<[u8; 4], SelectionReleaseErrorV1> {
    Ok(u32::try_from(value)
        .map_err(|_| SelectionReleaseErrorV1::ReleaseShapeOverflow)?
        .to_be_bytes())
}

/// Admit the exact authored release into its sealed canonical form.
///
/// Key order inside one tie group is not policy: groups are canonicalised by
/// sorting their keys byte-wise before the identity is computed, so authored
/// permutations inside a group seal identically. Any shape that cannot form a
/// total preorder is a typed rejection, never a panic.
pub(crate) fn admit_selection_release_v1(
    release: SelectionReleaseV1,
) -> Result<AdmittedSelectionReleaseV1, SelectionReleaseErrorV1> {
    if release.rank_groups.is_empty() {
        return Err(SelectionReleaseErrorV1::EmptyRelease);
    }
    let mut ranks: BTreeMap<Vec<u8>, usize> = BTreeMap::new();
    let mut hasher = sha256::Hasher::new();
    hasher.update(IDENTITY_DOMAIN_V1);
    hasher.update(&release.revision.to_be_bytes());
    hasher.update(&length_field_v1(release.rank_groups.len())?);
    for (rank, group) in release.rank_groups.iter().enumerate() {
        if group.is_empty() {
            return Err(SelectionReleaseErrorV1::EmptyRankGroup);
        }
        let mut keys = group
            .iter()
            .map(|key| key.as_bytes().to_vec())
            .collect::<Vec<_>>();
        keys.sort();
        hasher.update(&length_field_v1(keys.len())?);
        for key in keys {
            if key.is_empty() {
                return Err(SelectionReleaseErrorV1::EmptyCandidateKey);
            }
            if ranks.insert(key.clone(), rank).is_some() {
                return Err(SelectionReleaseErrorV1::DuplicateCandidateKey);
            }
            hasher.update(&length_field_v1(key.len())?);
            hasher.update(&key);
        }
    }
    Ok(AdmittedSelectionReleaseV1 {
        revision: release.revision,
        identity: hasher.finalize().as_bytes().to_owned(),
        ranks,
    })
}

impl AdmittedSelectionReleaseV1 {
    /// The content-addressed identity of the sealed release.
    pub(crate) fn identity(&self) -> [u8; 32] {
        self.identity
    }

    /// The release revision the identity is bound to.
    #[allow(dead_code)]
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    /// The preorder rank of one canonical key, when the release ranks it.
    pub(crate) fn rank_of(&self, key: &SelectionCandidateKeyV1) -> Option<usize> {
        self.ranks.get(key.as_bytes()).copied()
    }

    /// Materialise the total order of one candidate set from the release.
    ///
    /// Candidates are ordered by the admitted rank first and by the canonical
    /// key bytes second; nothing else is inspected. Every key must be ranked
    /// by the release and bound by exactly one candidate.
    pub(crate) fn select_order_v1<C: Clone>(
        &self,
        candidates: &[(C, SelectionCandidateKeyV1)],
    ) -> Result<Box<[C]>, SelectionReleaseErrorV1> {
        if candidates.is_empty() {
            return Err(SelectionReleaseErrorV1::EmptyCandidateSet);
        }
        let mut ranked = Vec::with_capacity(candidates.len());
        let mut bound: BTreeMap<&[u8], ()> = BTreeMap::new();
        for (payload, key) in candidates {
            let rank = self
                .ranks
                .get(key.as_bytes())
                .copied()
                .ok_or(SelectionReleaseErrorV1::UnknownCandidateKey)?;
            if bound.insert(key.as_bytes(), ()).is_some() {
                return Err(SelectionReleaseErrorV1::DuplicateCandidateBinding);
            }
            ranked.push((rank, key.as_bytes(), payload));
        }
        ranked.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(right.1)));
        Ok(ranked
            .into_iter()
            .map(|(_, _, payload)| payload.clone())
            .collect())
    }
}

/// Materialise the compiler-owned joint order from the sealed release (V5c-2).
///
/// The admitted release is the only authored selection authority: this
/// function derives the complete declared joint order from the release ranks
/// and the canonical key bytes alone, so binding order and declaration index
/// never participate and no evaluator ranks or selects.
pub(crate) fn materialise_joint_selection_v1(
    release: &AdmittedSelectionReleaseV1,
    bindings: &[(JointCandidateStateV1, SelectionCandidateKeyV1)],
) -> Result<DeclaredJointSelectionV1, SelectionReleaseErrorV1> {
    let states = release.select_order_v1(bindings)?;
    Ok(DeclaredJointSelectionV1::new(states.into_vec()))
}
