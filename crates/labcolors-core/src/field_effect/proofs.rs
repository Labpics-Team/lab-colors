//! Машинные границы поля: реальные функции, полный домен указанных целых.
//! Валидность extent в x86_64 допускает любые ненулевые u32 стороны.
use super::*;

fn valid_pixel(channels: [u8; 4]) -> bool {
    channels[0] <= channels[3] && channels[1] <= channels[3] && channels[2] <= channels[3]
}

#[kani::proof]
#[kani::unwind(5)]
fn premultiplied_admission_and_lighter_are_exact() {
    let s: [u8; 4] = kani::any();
    let d: [u8; 4] = kani::any();
    let source = PremultipliedRgba8V1::try_new(s);
    let destination = PremultipliedRgba8V1::try_new(d);
    assert!(
        source.is_ok() == valid_pixel(s) && destination.is_ok() == valid_pixel(d),
        "premultiplied admission must reject exactly channels above alpha"
    );
    if let (Ok(source), Ok(destination)) = (source, destination) {
        let lighter =
            porter_duff_lighter(source, destination).expect("valid premultiplied lighter is total");
        for c in 0..4 {
            let sum = u32::from(s[c]) + u32::from(d[c]);
            assert!(
                u32::from(lighter.channels()[c]) == sum.min(255),
                "lighter must clamp the exact sum of every channel including alpha"
            );
        }
        assert!(
            valid_pixel(lighter.channels()),
            "lighter channels must remain below their own alpha"
        );
        kani::cover!(s[3] == 0, "transparent lighter source");
        kani::cover!(
            u16::from(s[3]) + u16::from(d[3]) > 255,
            "lighter saturation"
        );
    }
    kani::cover!(!valid_pixel(s), "invalid premultiplied pixel");
}

#[kani::proof]
#[kani::unwind(5)]
fn premultiplied_source_over_has_unique_nearest_integer_output() {
    let s: [u8; 4] = kani::any();
    let d: [u8; 4] = kani::any();
    if !valid_pixel(s) || !valid_pixel(d) {
        return;
    }
    let source = PremultipliedRgba8V1(s);
    let destination = PremultipliedRgba8V1(d);
    let over = premultiplied_source_over(source, destination)
        .expect("valid premultiplied source-over is total");
    for c in 0..4 {
        // Нечётный знаменатель 255: единственный ближайший целый имеет
        // остаток не более 127. Oracle не повторяет деление production.
        let numerator = u32::from(s[c]) * 255 + u32::from(d[c]) * (255 - u32::from(s[3]));
        let rounded = u32::from(over.channels()[c]) * 255;
        assert!(
            rounded <= numerator + 127 && numerator <= rounded + 127,
            "premultiplied source-over must return the unique nearest rational value"
        );
    }
    assert!(
        valid_pixel(over.channels()),
        "source-over channels must remain below their own alpha"
    );
    kani::cover!(s[3] == 0, "transparent premultiplied source");
    kani::cover!(s[3] == 255, "opaque premultiplied source");
    kani::cover!(s[3] > 0 && s[3] < 255 && d[3] > 0, "two translucent pixels");
}

#[kani::proof]
fn rectangles_admit_exactly_nonempty_in_bounds_geometry() {
    let extent = FieldExtentV1 {
        width: kani::any(),
        height: kani::any(),
    };
    if extent.width == 0 || extent.height == 0 {
        return;
    }
    let x: u32 = kani::any();
    let y: u32 = kani::any();
    let width: u32 = kani::any();
    let height: u32 = kani::any();
    let right = u64::from(x) + u64::from(width);
    let bottom = u64::from(y) + u64::from(height);
    let valid = width != 0
        && height != 0
        && right <= u64::from(extent.width)
        && bottom <= u64::from(extent.height);
    let actual = FieldRectV1::try_new(extent, x, y, width, height);
    assert!(
        actual.is_ok() == valid,
        "field rectangle must admit exactly its nonempty bounded geometry"
    );
    if let Ok(rect) = actual {
        assert!(
            rect.extent() == extent
                && rect.x() == x
                && rect.y() == y
                && rect.width() == width
                && rect.height() == height,
            "rectangle admission must preserve all declared coordinates"
        );
    }
    kani::cover!(
        valid && right == u64::from(extent.width),
        "rectangle touches right edge"
    );
    kani::cover!(width == 0 || height == 0, "empty rectangle rejected");
    kani::cover!(
        right > u64::from(u32::MAX),
        "rectangle addition overflow rejected"
    );
    kani::cover!(
        width > 0 && right > u64::from(extent.width) && right <= u64::from(u32::MAX),
        "outside rectangle rejected"
    );
}

#[kani::proof]
fn expanded_rectangles_preserve_exact_clipped_influence() {
    let extent = FieldExtentV1 {
        width: kani::any(),
        height: kani::any(),
    };
    if extent.width == 0 || extent.height == 0 {
        return;
    }
    let x: u32 = kani::any();
    let y: u32 = kani::any();
    let width: u32 = kani::any();
    let height: u32 = kani::any();
    if width == 0
        || height == 0
        || u64::from(x) + u64::from(width) > u64::from(extent.width)
        || u64::from(y) + u64::from(height) > u64::from(extent.height)
    {
        return;
    }
    // Представление непосредственно задаёт весь admitted domain, не фильтр по результату expanded.
    let rect = FieldRectV1 {
        extent,
        x,
        y,
        width,
        height,
    };
    let radius: u32 = kani::any();
    let requested = if kani::any() {
        extent
    } else {
        FieldExtentV1 {
            width: kani::any(),
            height: kani::any(),
        }
    };
    if requested.width == 0 || requested.height == 0 {
        return;
    }
    let right = u64::from(x) + u64::from(width) + u64::from(radius);
    let bottom = u64::from(y) + u64::from(height) + u64::from(radius);
    let overflow = right > u64::from(u32::MAX) || bottom > u64::from(u32::MAX);
    let result = rect.expanded(radius, requested);
    if requested != extent {
        assert!(
            result == Err(FieldEvaluationErrorV1::ExtentMismatch),
            "field expansion must reject a foreign extent"
        );
    } else if overflow {
        assert!(
            result == Err(FieldEvaluationErrorV1::GeometryOverflow),
            "clipping must not hide overflowing declared geometry"
        );
    } else {
        let expanded = result.expect("representable same-extent expansion must succeed");
        let left = (i64::from(x) - i64::from(radius)).max(0) as u32;
        let top = (i64::from(y) - i64::from(radius)).max(0) as u32;
        let end_x = right.min(u64::from(extent.width)) as u32;
        let end_y = bottom.min(u64::from(extent.height)) as u32;
        assert!(
            expanded.extent() == extent
                && expanded.x() == left
                && expanded.y() == top
                && expanded.width() == end_x - left
                && expanded.height() == end_y - top,
            "field expansion must contain exactly the clipped radius on every edge"
        );
        kani::cover!(radius == 0, "zero radius is identity");
        kani::cover!(
            radius > x && right > u64::from(extent.width),
            "both horizontal edges clipped"
        );
    }
    kani::cover!(
        requested == extent && overflow,
        "expansion overflow rejected"
    );
    kani::cover!(requested != extent, "foreign expansion extent rejected");
}

#[kani::proof]
fn gaussian_sampling_clamps_without_coordinate_wrap() {
    let limit: u32 = kani::any();
    let coordinate: u32 = kani::any();
    let radius: u32 = kani::any();
    let index: usize = kani::any();
    if limit == 0 || coordinate >= limit {
        return;
    }
    let exact = i128::from(coordinate) + index as i128 - i128::from(radius);
    let overflow = index as u128 > i64::MAX as u128 || exact > i128::from(i64::MAX);
    let actual = gaussian_sample_coordinate(
        coordinate,
        index,
        radius,
        limit,
        GaussianEdgeModeV1::ClampToEdgeV1,
    );
    if overflow {
        assert!(
            actual == Err(FieldEvaluationErrorV1::GeometryOverflow),
            "gaussian sample must reject unrepresentable index or coordinate"
        );
    } else {
        let expected = exact.max(0).min(i128::from(limit) - 1) as u32;
        assert!(
            actual == Ok(expected),
            "gaussian sample must equal exact clamped coordinate"
        );
        kani::cover!(exact < 0, "gaussian left edge clamp");
        kani::cover!(exact >= i128::from(limit), "gaussian right edge clamp");
        kani::cover!(
            exact >= 0 && exact < i128::from(limit),
            "gaussian interior sample"
        );
    }
    kani::cover!(overflow, "gaussian index overflow");
}
