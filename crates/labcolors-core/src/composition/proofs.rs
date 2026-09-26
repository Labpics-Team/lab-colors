//! Binary64-домен: проверки битов независимы от float-условий production.
use super::*;

fn admitted_bits(bits: u64) -> bool {
    bits <= 1.0_f64.to_bits() || bits == (-0.0_f64).to_bits()
}

#[kani::proof]
fn opacity_admission_is_exact_and_canonical() {
    let bits: u64 = kani::any();
    let result = AdmittedOpacityV1::new(f64::from_bits(bits));
    assert!(
        result.is_ok() == admitted_bits(bits),
        "opacity must admit exactly finite unit binary64 values"
    );
    if let Ok(alpha) = result {
        let expected = if bits == (-0.0_f64).to_bits() {
            0
        } else {
            bits
        };
        assert!(
            alpha.bits() == expected,
            "opacity must preserve every nonzero bit and canonicalize zero"
        );
        assert!(
            alpha.value().to_bits() == expected,
            "opacity value must agree with its canonical bits"
        );
        match alpha.predecessor() {
            None => {
                assert!(
                    expected == 0,
                    "opacity predecessor must be the previous admissible value"
                );
            }
            Some(prev) => {
                assert!(
                    prev.bits().checked_add(1) == Some(expected) && admitted_bits(prev.bits()),
                    "opacity predecessor must be the previous admissible value"
                );
            }
        }
    }
    kani::cover!(bits == (-0.0_f64).to_bits(), "negative zero");
    kani::cover!(bits == 1, "smallest subnormal");
    kani::cover!(result.is_err(), "invalid opacity");
    kani::cover!(result.is_ok(), "admitted opacity");
}

#[kani::proof]
fn opacity_domain_preserves_all_boundaries() {
    let lower: u64 = kani::any();
    let upper: u64 = kani::any();
    let query: u64 = kani::any();
    let canonical = |b| if b == (-0.0_f64).to_bits() { 0 } else { b };
    let valid =
        admitted_bits(lower) && admitted_bits(upper) && canonical(lower) <= canonical(upper);
    let result = OpacityDomainV1::try_new(f64::from_bits(lower), f64::from_bits(upper));
    assert!(
        result.is_ok() == valid,
        "opacity domain admission must reject invalid or reversed endpoints"
    );
    if let Ok(domain) = result {
        assert!(
            domain.lower().bits() == canonical(lower) && domain.upper().bits() == canonical(upper)
        );
        if let Ok(q) = AdmittedOpacityV1::new(f64::from_bits(query)) {
            assert!(
                domain.contains(q)
                    == (canonical(lower) <= canonical(query)
                        && canonical(query) <= canonical(upper)),
                "opacity membership must include exactly the closed interval"
            );
        }
    }
    kani::cover!(
        valid && canonical(lower) == canonical(upper),
        "fixed opacity domain"
    );
    kani::cover!(
        valid && canonical(lower) < canonical(upper),
        "nontrivial opacity domain"
    );
    kani::cover!(!valid, "invalid opacity domain");
}

#[kani::proof]
fn opacity_multiplication_preserves_identity_and_zero() {
    let Ok(alpha) = AdmittedOpacityV1::new(kani::any()) else {
        return;
    };
    assert!(
        alpha.multiply(AdmittedOpacityV1::OPAQUE) == alpha
            && AdmittedOpacityV1::OPAQUE.multiply(alpha) == alpha,
        "unit opacity must be an exact multiplicative identity"
    );
    assert!(
        alpha.multiply(AdmittedOpacityV1::TRANSPARENT) == AdmittedOpacityV1::TRANSPARENT
            && AdmittedOpacityV1::TRANSPARENT.multiply(alpha) == AdmittedOpacityV1::TRANSPARENT,
        "zero opacity must remain canonical under multiplication"
    );
    kani::cover!(alpha.bits() == 1, "subnormal opacity identity");
    kani::cover!(alpha.bits() == 1.0_f64.to_bits(), "opaque identity");
}

#[kani::proof]
fn source_over_endpoints_preserve_channel_values() {
    let tint: u8 = kani::any();
    let backdrop: u8 = kani::any();
    assert!(
        source_over_channel_srgb8(tint, 0.0, backdrop) == backdrop,
        "transparent source must preserve every backdrop channel"
    );
    assert!(
        source_over_channel_srgb8(tint, 1.0, backdrop) == tint,
        "opaque source must preserve every source channel"
    );
    kani::cover!(tint < backdrop, "darkening endpoints");
    kani::cover!(tint > backdrop, "lightening endpoints");
}

#[kani::proof]
fn identical_channels_are_fixed_for_every_opacity() {
    let channel: u8 = kani::any();
    let bits: u64 = kani::any();
    if let Ok(opacity) = AdmittedOpacityV1::new(f64::from_bits(bits)) {
        assert!(
            source_over_channel_srgb8(channel, opacity.value(), channel) == channel,
            "equal source and backdrop must be a fixed point for every valid opacity"
        );
        kani::cover!(opacity.bits() == 1, "subnormal fixed point");
        kani::cover!(
            opacity.value() > 0.0 && opacity.value() < 1.0,
            "nonendpoint fixed point"
        );
    }
}

// Начальное состояние точно соответствует каноническому admitted-domain:
// new() и канонизация -0 уже доказаны отдельно; ни один допустимый бит не исключён.
#[kani::proof]
fn multiply_preserves_the_entire_admitted_domain() {
    let left: u64 = kani::any();
    let right: u64 = kani::any();
    if left > AdmittedOpacityV1::OPAQUE.bits() || right > AdmittedOpacityV1::OPAQUE.bits() {
        return;
    }
    let result = AdmittedOpacityV1(left).multiply(AdmittedOpacityV1(right));
    assert!(
        result.bits() <= AdmittedOpacityV1::OPAQUE.bits(),
        "multiplying any admitted opacities must preserve the entire canonical unit domain"
    );
    kani::cover!(
        left > 0 && right > 0 && result.bits() == 0,
        "positive product underflow"
    );
    kani::cover!(
        result.bits() > 0 && result.bits() < (1_u64 << 52),
        "subnormal product"
    );
    kani::cover!(
        left == AdmittedOpacityV1::OPAQUE.bits() && right == left,
        "opaque product"
    );
}

#[kani::proof]
fn source_over_is_bounded_before_quantization() {
    let source: u8 = kani::any();
    let backdrop: u8 = kani::any();
    let bits: u64 = kani::any();
    if bits > AdmittedOpacityV1::OPAQUE.bits() {
        return;
    }
    let value = source_over_channel_value(source, f64::from_bits(bits), backdrop);
    assert!(
        value.to_bits() <= 255.0_f64.to_bits(),
        "every source-over value must be finite and in byte range before quantization"
    );
    kani::cover!(
        source < backdrop && bits > 0 && bits < AdmittedOpacityV1::OPAQUE.bits(),
        "intermediate darkening"
    );
    kani::cover!(
        source > backdrop && bits > 0 && bits < AdmittedOpacityV1::OPAQUE.bits(),
        "intermediate lightening"
    );
}
