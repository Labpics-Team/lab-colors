use labcolors_core::LcsColor;
use labcolors_core::neutral::NeutralCurve;

fn srgb_decode(encoded: f64) -> f64 {
    if encoded <= 0.040_45 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

fn srgb_encode(linear: f64) -> f64 {
    if linear > 0.003_130_8 {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    } else {
        12.92 * linear
    }
}

fn decode_byte(byte: u8) -> f64 {
    srgb_decode(f64::from(byte) / 255.0)
}

fn encode_byte(linear: f64) -> u8 {
    (srgb_encode(linear).clamp(0.0, 1.0) * 255.0).round() as u8
}

fn first_transition_t(from: u8, to: u8, target: u8) -> f64 {
    let start = decode_byte(from);
    let end = decode_byte(to);
    let crossed = |t: f64| {
        let byte = encode_byte(start + (end - start) * t);
        if to > from {
            byte >= target
        } else {
            byte <= target
        }
    };

    assert!(!crossed(0.0));
    assert!(crossed(1.0));

    // Для положительных finite f64 порядок битов совпадает с числовым. Поэтому
    // бинарный поиск идёт по ВСЕМ представимым t, а не по приближённой сетке.
    let mut lo = 0.0_f64.to_bits();
    let mut hi = 1.0_f64.to_bits();
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if crossed(f64::from_bits(mid)) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    f64::from_bits(hi)
}

#[derive(Debug, Clone, Copy)]
struct Event {
    t: f64,
    channel: usize,
    next_byte: u8,
}

/// Независимый reference конечного пути: каждое событие ищется через реальный
/// encode+round production-контракт на полном множестве представимых f64 `t`.
fn enumerate_live_segment(from: [u8; 3], to: [u8; 3]) -> Vec<[u8; 3]> {
    let mut events = Vec::new();
    for channel in 0..3 {
        match to[channel].cmp(&from[channel]) {
            std::cmp::Ordering::Greater => {
                for next_byte in (from[channel] + 1)..=to[channel] {
                    events.push(Event {
                        t: first_transition_t(from[channel], to[channel], next_byte),
                        channel,
                        next_byte,
                    });
                }
            }
            std::cmp::Ordering::Less => {
                for next_byte in to[channel]..from[channel] {
                    events.push(Event {
                        t: first_transition_t(from[channel], to[channel], next_byte),
                        channel,
                        next_byte,
                    });
                }
            }
            std::cmp::Ordering::Equal => {}
        }
    }
    events.sort_by(|a, b| a.t.total_cmp(&b.t).then_with(|| a.channel.cmp(&b.channel)));

    let mut bytes = from;
    let mut states = vec![bytes];
    let mut index = 0;
    while index < events.len() {
        let event_t = events[index].t;
        while index < events.len() && events[index].t == event_t {
            let event = events[index];
            bytes[event.channel] = event.next_byte;
            index += 1;
        }
        states.push(bytes);
    }
    assert_eq!(bytes, to);
    states
}

fn hex([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

#[test]
fn finite_neutral_path_uses_live_quantizer_not_inverse_half_walls() {
    const LIGHT: [u8; 3] = [9, 10, 0];
    const BASE: [u8; 3] = [0, 1, 0];
    const DARK: [u8; 3] = [0, 0, 0];
    const LOST: [u8; 3] = [9, 9, 0];

    let mut path = enumerate_live_segment(LIGHT, BASE);
    path.extend(enumerate_live_segment(BASE, DARK).into_iter().skip(1));

    let lost_index = path
        .iter()
        .position(|&state| state == LOST)
        .expect("live encode+round path обязан содержать #090900");
    assert!(lost_index > 0 && lost_index + 1 < path.len());

    let colors: Vec<LcsColor> = path
        .iter()
        .map(|&state| LcsColor::from_hex(&hex(state)).unwrap())
        .collect();
    let mut cumulative = Vec::with_capacity(colors.len());
    let mut total = 0.0_f64;
    let mut compensation = 0.0_f64;
    cumulative.push(0.0);
    for pair in colors.windows(2) {
        let step = pair[0].delta_e_ucs(&pair[1]);
        let corrected = step - compensation;
        let next = total + corrected;
        compensation = (next - total) - corrected;
        total = next;
        cumulative.push(total);
    }

    // Центр Voronoi-интервала LOST по правильной накопленной длине: корректная
    // finite-кривая обязана выбрать именно это состояние, без пограничного tie.
    let left = 0.5 * (cumulative[lost_index - 1] + cumulative[lost_index]);
    let right = 0.5 * (cumulative[lost_index] + cumulative[lost_index + 1]);
    let t = 0.5 * (left + right) / total;

    let curve = NeutralCurve::new(&hex(LIGHT), &hex(BASE), &hex(DARK)).unwrap();
    assert_eq!(
        curve.at(t).to_hex(),
        hex(LOST),
        "inverse-half-wall enumeration потеряло реально эмитируемое состояние"
    );
}
