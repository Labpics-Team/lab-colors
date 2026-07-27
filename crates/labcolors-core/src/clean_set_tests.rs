use crate::Srgb8;
use crate::clean_set::{
    EXACT_NOMINAL_SRGB8_CLEAN_SET_ACCEPTED_COUNT_V1, EXACT_NOMINAL_SRGB8_CLEAN_SET_CODEC_SHA256_V1,
    EXACT_NOMINAL_SRGB8_CLEAN_SET_RAW_TABLE_SHA256_V1, ExactNominalSrgb8CleanSetDecisionV1,
    ExactNominalSrgb8CleanSetV1, RejectedBlueIntervalV1, exact_nominal_srgb8_clean_set_codec_v1,
};
use crate::sha256::Hasher;

#[test]
fn neutral_axis_precedes_declared_rejected_interval() {
    let profile = ExactNominalSrgb8CleanSetV1;

    assert_eq!(
        profile.classify(Srgb8::new([0x80, 0x80, 0x80])),
        ExactNominalSrgb8CleanSetDecisionV1::Accepted,
    );
    assert!(matches!(
        profile.classify(Srgb8::new([0x80, 0x80, 0x81])),
        ExactNominalSrgb8CleanSetDecisionV1::Rejected(_),
    ));
}

#[test]
fn closed_interval_endpoints_are_rejected() {
    let profile = ExactNominalSrgb8CleanSetV1;
    let interval = profile.rejected_blue_interval(0, 200);

    assert_eq!(interval, RejectedBlueIntervalV1::Closed { lo: 71, hi: 101 },);
    for blue in [70, 71, 101, 102] {
        let decision = profile.classify(Srgb8::new([0, 200, blue]));
        assert_eq!(
            matches!(decision, ExactNominalSrgb8CleanSetDecisionV1::Rejected(_)),
            (71..=101).contains(&blue),
            "closed-boundary semantics drifted at blue={blue}",
        );
        if let ExactNominalSrgb8CleanSetDecisionV1::Rejected(interval) = decision {
            assert_eq!(interval.endpoints(), [71, 101]);
        }
    }
}

#[test]
fn embedded_codec_has_the_package_pinned_content_identity() {
    let codec = exact_nominal_srgb8_clean_set_codec_v1();
    assert_eq!(&codec[..8], b"LPCC\x01\x01\x00\x00");

    let mut digest = Hasher::new();
    digest.update(codec);
    assert_eq!(
        digest.finalize().as_bytes(),
        &EXACT_NOMINAL_SRGB8_CLEAN_SET_CODEC_SHA256_V1,
    );
}

#[test]
fn table_none_sentinel_is_not_absent_final_owned_domain() {
    let profile = ExactNominalSrgb8CleanSetV1;

    assert_eq!(
        profile.rejected_blue_interval(255, 0),
        RejectedBlueIntervalV1::None,
    );
    for blue in u8::MIN..=u8::MAX {
        assert_eq!(
            profile.classify(Srgb8::new([255, 0, blue])),
            ExactNominalSrgb8CleanSetDecisionV1::Accepted,
        );
    }
}

#[test]
fn runtime_classifier_matches_the_content_bound_total_table() {
    let profile = ExactNominalSrgb8CleanSetV1;
    let mut accepted = 0_u32;
    let mut raw_table = Hasher::new();

    for red in u8::MIN..=u8::MAX {
        for green in u8::MIN..=u8::MAX {
            let interval = profile.rejected_blue_interval(red, green);
            raw_table.update(&interval.raw_pair_v1());
            for blue in u8::MIN..=u8::MAX {
                let color = Srgb8::new([red, green, blue]);
                let expected = red == green && green == blue || !interval.contains_closed(blue);
                let actual = profile.classify(color);
                assert_eq!(
                    matches!(actual, ExactNominalSrgb8CleanSetDecisionV1::Accepted),
                    expected,
                    "classifier/table mismatch at {color:?}",
                );
                accepted += u32::from(expected);
            }
        }
    }

    assert_eq!(
        raw_table.finalize().as_bytes(),
        &EXACT_NOMINAL_SRGB8_CLEAN_SET_RAW_TABLE_SHA256_V1,
    );
    assert_eq!(accepted, EXACT_NOMINAL_SRGB8_CLEAN_SET_ACCEPTED_COUNT_V1,);
}
