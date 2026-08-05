//! Hostile contract for the sole authored selection release (V5c-1).
//!
//! One versioned `SelectionReleaseV1` is the only authored selection input:
//! it declares a total preorder over opaque canonical candidate keys as an
//! ordered sequence of tie groups, and the single common tie-break inside a
//! group is the canonical key bytes themselves. Declaration index, `usize`
//! positions, RGB bytes, distances and weights never participate in the
//! order. Admission seals the release once and content-addresses it; any
//! release shape that cannot form a total preorder is a typed rejection,
//! and selection materialisation never sorts by anything but the admitted
//! rank and the canonical key.

use crate::selection_release::{
    SelectionCandidateKeyV1, SelectionReleaseErrorV1, SelectionReleaseV1,
    admit_selection_release_v1,
};

fn key(bytes: &[u8]) -> SelectionCandidateKeyV1 {
    SelectionCandidateKeyV1::new(bytes.to_vec().into_boxed_slice())
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
fn admission_rejects_every_foreign_release_shape() {
    // no rank groups at all
    let rejected = admit_selection_release_v1(release(1, &[]));
    assert!(matches!(
        rejected,
        Err(SelectionReleaseErrorV1::EmptyRelease)
    ));
    // one empty tie group
    let rejected = admit_selection_release_v1(release(1, &[&[]]));
    assert!(matches!(
        rejected,
        Err(SelectionReleaseErrorV1::EmptyRankGroup)
    ));
    // an empty canonical key
    let rejected = admit_selection_release_v1(release(1, &[&[b""]]));
    assert!(matches!(
        rejected,
        Err(SelectionReleaseErrorV1::EmptyCandidateKey)
    ));
    // duplicate key inside one tie group
    let rejected = admit_selection_release_v1(release(1, &[&[b"a", b"a"]]));
    assert!(matches!(
        rejected,
        Err(SelectionReleaseErrorV1::DuplicateCandidateKey)
    ));
    // duplicate key across two tie groups
    let rejected = admit_selection_release_v1(release(1, &[&[b"a"], &[b"a"]]));
    assert!(matches!(
        rejected,
        Err(SelectionReleaseErrorV1::DuplicateCandidateKey)
    ));
}

#[test]
fn identity_is_content_addressed_and_revision_bound() {
    let first = admit_selection_release_v1(release(7, &[&[b"a", b"b"], &[b"c"]]))
        .expect("authored release must admit");
    let second = admit_selection_release_v1(release(7, &[&[b"a", b"b"], &[b"c"]]))
        .expect("the same authoring must admit identically");
    assert_eq!(first.identity(), second.identity());
    let renumbered = admit_selection_release_v1(release(8, &[&[b"a", b"b"], &[b"c"]]))
        .expect("a renumbered release must still admit");
    assert_ne!(first.identity(), renumbered.identity());
}

#[test]
fn identity_length_fields_separate_key_and_group_boundaries() {
    let joined =
        admit_selection_release_v1(release(1, &[&[b"ab"]])).expect("single joined key must admit");
    let split = admit_selection_release_v1(release(1, &[&[b"a", b"b"]]))
        .expect("split keys with the same joined bytes must admit");
    assert_ne!(joined.identity(), split.identity());
    let left_split = admit_selection_release_v1(release(1, &[&[b"ab", b"c"]]))
        .expect("left key split must admit");
    let right_split = admit_selection_release_v1(release(1, &[&[b"a", b"bc"]]))
        .expect("right key split with the same joined bytes must admit");
    assert_ne!(left_split.identity(), right_split.identity());
}

#[test]
fn key_permutation_inside_a_tie_group_is_not_policy() {
    let canonical = admit_selection_release_v1(release(1, &[&[b"zeta", b"alpha"], &[b"beta"]]))
        .expect("authored release must admit");
    let permuted = admit_selection_release_v1(release(1, &[&[b"alpha", b"zeta"], &[b"beta"]]))
        .expect("permuted tie group must admit");
    assert_eq!(canonical.identity(), permuted.identity());
    let candidates = [(1u32, key(b"zeta")), (2, key(b"alpha")), (3, key(b"beta"))];
    assert_eq!(
        canonical.select_order_v1(&candidates).unwrap(),
        permuted.select_order_v1(&candidates).unwrap()
    );
}

#[test]
fn total_preorder_ranks_follow_the_authored_groups() {
    let admitted = admit_selection_release_v1(release(1, &[&[b"x", b"y"], &[b"z"]]))
        .expect("authored release must admit");
    assert_eq!(admitted.rank_of(&key(b"x")), Some(0));
    assert_eq!(admitted.rank_of(&key(b"y")), Some(0));
    assert_eq!(admitted.rank_of(&key(b"z")), Some(1));
    assert_eq!(admitted.rank_of(&key(b"foreign")), None);
}

#[test]
fn selection_orders_by_rank_then_canonical_key_bytes_only() {
    let admitted = admit_selection_release_v1(release(1, &[&[b"zz", b"aa"], &[b"mm"], &[b"bb"]]))
        .expect("authored release must admit");
    let candidates = [
        ("last-declared", key(b"mm")),
        ("first-declared", key(b"zz")),
        ("middle-declared", key(b"aa")),
        ("tail-declared", key(b"bb")),
    ];
    let selected = admitted.select_order_v1(&candidates).unwrap();
    // rank dominates, and the tie inside rank 0 breaks on key bytes, never on
    // declaration position
    assert_eq!(
        selected.as_ref(),
        [
            "middle-declared",
            "first-declared",
            "last-declared",
            "tail-declared"
        ]
    );
    // permuting the candidate input order never changes the selection order
    let shuffled = [
        ("tail-declared", key(b"bb")),
        ("middle-declared", key(b"aa")),
        ("last-declared", key(b"mm")),
        ("first-declared", key(b"zz")),
    ];
    assert_eq!(admitted.select_order_v1(&shuffled).unwrap(), selected);
}

#[test]
fn selection_rejects_every_foreign_binding() {
    let admitted = admit_selection_release_v1(release(1, &[&[b"a"], &[b"b"]]))
        .expect("authored release must admit");
    // unknown key is not silently ranked
    let rejected = admitted.select_order_v1(&[(1u32, key(b"foreign"))]);
    assert!(matches!(
        rejected,
        Err(SelectionReleaseErrorV1::UnknownCandidateKey)
    ));
    // two candidates bound to one canonical key receive no hidden merge
    let rejected = admitted.select_order_v1(&[(1u32, key(b"a")), (2, key(b"a"))]);
    assert!(matches!(
        rejected,
        Err(SelectionReleaseErrorV1::DuplicateCandidateBinding)
    ));
    // an empty candidate set never produces a selection
    let rejected = admitted.select_order_v1::<u32>(&[]);
    assert!(matches!(
        rejected,
        Err(SelectionReleaseErrorV1::EmptyCandidateSet)
    ));
}

#[test]
fn bijective_payload_relabeling_preserves_the_order_structure() {
    let admitted = admit_selection_release_v1(release(1, &[&[b"k1", b"k2"], &[b"k3"]]))
        .expect("authored release must admit");
    let candidates = [(10u32, key(b"k2")), (20, key(b"k3")), (30, key(b"k1"))];
    let original = admitted.select_order_v1(&candidates).unwrap();
    let relabeled_candidates = candidates
        .iter()
        .map(|(payload, key)| (payload.wrapping_mul(31).wrapping_add(7), key.clone()))
        .collect::<Vec<_>>();
    let relabeled = admitted.select_order_v1(&relabeled_candidates).unwrap();
    let expected = original
        .iter()
        .map(|payload| payload.wrapping_mul(31).wrapping_add(7))
        .collect::<Vec<_>>();
    assert_eq!(relabeled.as_ref(), expected.as_slice());
}
