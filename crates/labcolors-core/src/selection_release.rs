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
    /// Хотя бы один ключ выпуска не связан ни с одним кандидатом.
    MissingCandidateBinding,
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

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Идентификатор содержимого одного допущенного авторского выпуска выбора.
///
/// У байтов нет публичного конструктора: production-код получает значение
/// только при допуске выпуска и передаёт его в Program лишь вместе с
/// материализованным порядком ниже.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SelectionReleaseIdentityV1([u8; 32]);

impl SelectionReleaseIdentityV1 {
    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
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
    identity: SelectionReleaseIdentityV1,
    ranks: BTreeMap<Vec<u8>, usize>,
}

/// Единственный production-вход конечного выбора в Program.
///
/// Оба поля неизменяемы и закрыты. До компиляции проверенный порядок нельзя
/// отделить от точного выпуска, который его задал.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MaterialisedSelectionV1 {
    release_identity: SelectionReleaseIdentityV1,
    order: DeclaredJointSelectionV1,
}

impl MaterialisedSelectionV1 {
    pub(crate) const fn release_identity(&self) -> SelectionReleaseIdentityV1 {
        self.release_identity
    }

    pub(crate) const fn order(&self) -> &DeclaredJointSelectionV1 {
        &self.order
    }
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
        identity: SelectionReleaseIdentityV1(*hasher.finalize().as_bytes()),
        ranks,
    })
}

impl AdmittedSelectionReleaseV1 {
    /// The content-addressed identity of the sealed release.
    pub(crate) const fn identity(&self) -> SelectionReleaseIdentityV1 {
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
        if bound.len() != self.ranks.len() {
            return Err(SelectionReleaseErrorV1::MissingCandidateBinding);
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
) -> Result<MaterialisedSelectionV1, SelectionReleaseErrorV1> {
    let states = release.select_order_v1(bindings)?;
    let order = states
        .into_vec()
        .into_iter()
        .map(JointCandidateStateV1::canonicalise_keyed_choices)
        .collect();
    Ok(MaterialisedSelectionV1 {
        release_identity: release.identity(),
        order: DeclaredJointSelectionV1::new(order),
    })
}

/// Шов совместимости для тестовых фикстур, созданных до `SelectionRelease`.
///
/// Он не подделывает запечатанную пару: каждый переданный порядок связывается
/// с допущенным синтетическим выпуском и проходит production-материализацию.
#[cfg(test)]
pub(crate) fn materialise_declared_joint_selection_for_test(
    order: DeclaredJointSelectionV1,
) -> MaterialisedSelectionV1 {
    let rank_groups = (0_u64..)
        .zip(order.states())
        .map(|(index, _)| {
            vec![SelectionCandidateKeyV1::new(
                index.to_be_bytes().to_vec().into_boxed_slice(),
            )]
            .into_boxed_slice()
        })
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let release = admit_selection_release_v1(SelectionReleaseV1::new(0, rank_groups))
        .expect("a non-empty generated test release must admit");
    let bindings = (0_u64..)
        .zip(order.states())
        .map(|(index, state)| {
            (
                state.clone(),
                SelectionCandidateKeyV1::new(index.to_be_bytes().to_vec().into_boxed_slice()),
            )
        })
        .collect::<Vec<_>>();
    materialise_joint_selection_v1(&release, &bindings)
        .expect("an admitted generated test release must materialise its complete bindings")
}
