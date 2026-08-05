//! Hostile contract for the compiler-owned joint order materialisation
//! (V5c-2).
//!
//! The admitted selection release is the only authored selection authority:
//! the complete joint order is materialised from the release ranks and the
//! canonical key bytes alone. Binding order, declaration index and `usize`
//! positions never participate, and every foreign binding shape is a typed
//! rejection.

use crate::program_session::{
    JointCandidateStateV1, TargetCandidateChoiceV1, TargetCandidateId, TargetId,
};
use crate::selection_release::{
    SelectionCandidateKeyV1, SelectionReleaseErrorV1, SelectionReleaseV1,
    admit_selection_release_v1, materialise_joint_selection_v1,
};

fn key(bytes: &[u8]) -> SelectionCandidateKeyV1 {
    SelectionCandidateKeyV1::new(bytes.to_vec().into_boxed_slice())
}

fn state(target: u32, candidate: u32) -> JointCandidateStateV1 {
    JointCandidateStateV1::new(vec![TargetCandidateChoiceV1::new(
        TargetId::new(target),
        TargetCandidateId::new(candidate),
    )])
}

fn release(revision: u64, groups: &[&[&[u8]]]) -> SelectionReleaseV1 {
    SelectionReleaseV1::new(
        revision,
        groups
            .iter()
            .map(|group| {
                group
                    .iter()
                    .map(|bytes| key(bytes))
                    .collect::<Vec<_>>()
                    .into_boxed_slice()
            })
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    )
}

#[test]
fn materialised_joint_order_follows_the_release_not_the_binding_order() {
    let admitted = admit_selection_release_v1(release(1, &[&[b"win"], &[b"lose"]]))
        .expect("authored release must admit");
    let loser = state(1, 10);
    let winner = state(1, 11);
    let selection = materialise_joint_selection_v1(
        &admitted,
        &[(loser.clone(), key(b"lose")), (winner.clone(), key(b"win"))],
    )
    .expect("complete binding must materialise");
    assert_eq!(selection.states(), &[winner, loser]);
}

#[test]
fn binding_permutation_is_not_policy() {
    let admitted = admit_selection_release_v1(release(1, &[&[b"one"], &[b"two"], &[b"three"]]))
        .expect("authored release must admit");
    let first = state(1, 1);
    let second = state(1, 2);
    let third = state(1, 3);
    let canonical = materialise_joint_selection_v1(
        &admitted,
        &[
            (first.clone(), key(b"one")),
            (second.clone(), key(b"two")),
            (third.clone(), key(b"three")),
        ],
    )
    .expect("complete binding must materialise");
    let permuted = materialise_joint_selection_v1(
        &admitted,
        &[
            (third.clone(), key(b"three")),
            (first.clone(), key(b"one")),
            (second.clone(), key(b"two")),
        ],
    )
    .expect("permuted binding must materialise identically");
    assert_eq!(canonical, permuted);
    assert_eq!(canonical.states(), &[first, second, third]);
}

#[test]
fn tie_group_materialises_by_canonical_key_bytes_only() {
    let admitted = admit_selection_release_v1(release(1, &[&[b"bb", b"aa"], &[b"cc"]]))
        .expect("authored release must admit");
    let bb = state(1, 20);
    let aa = state(1, 21);
    let cc = state(1, 22);
    let selection = materialise_joint_selection_v1(
        &admitted,
        &[
            (bb.clone(), key(b"bb")),
            (cc.clone(), key(b"cc")),
            (aa.clone(), key(b"aa")),
        ],
    )
    .expect("complete binding must materialise");
    assert_eq!(selection.states(), &[aa, bb, cc]);
}

#[test]
fn materialisation_rejects_every_foreign_binding_shape() {
    let admitted = admit_selection_release_v1(release(1, &[&[b"known"]]))
        .expect("authored release must admit");
    let known = state(1, 1);
    assert_eq!(
        materialise_joint_selection_v1(&admitted, &[]),
        Err(SelectionReleaseErrorV1::EmptyCandidateSet)
    );
    assert_eq!(
        materialise_joint_selection_v1(&admitted, &[(known.clone(), key(b"foreign"))]),
        Err(SelectionReleaseErrorV1::UnknownCandidateKey)
    );
    assert_eq!(
        materialise_joint_selection_v1(
            &admitted,
            &[(known.clone(), key(b"known")), (state(1, 2), key(b"known"))]
        ),
        Err(SelectionReleaseErrorV1::DuplicateCandidateBinding)
    );
}
