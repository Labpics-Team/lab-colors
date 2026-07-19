//! Published anchors and frozen characterization for the FNV-1a 32-bit core primitive.
//!
//! `tests/data/fnv1a-vectors.txt` is an LF-pinned TSV. Every row is recomputed
//! against its committed unsigned `u32`: empty string, Cyrillic, emoji,
//! high-bit bytes, an overflow-length key, and a 500-vector randomized corpus.
//!
//! `anchor` rows carry the CANONICAL published FNV-1a values (external ground
//! truth, <http://www.isthe.com/chongo/tech/comp/fnv/>) so correctness is
//! grounded in the spec, not self-blessed. The remaining committed expected
//! values have no preserved independent provenance and therefore protect only
//! frozen behaviour. `text` rows are literal strings so Rust exercises its own
//! UTF-8 encoding path.

use labcolors_core::fnv1a_32;

struct Vector {
    group: String,
    name: String,
    bytes: Vec<u8>,
    expected: u32,
}

fn load() -> Vec<Vector> {
    let raw = include_str!("data/fnv1a-vectors.txt");
    raw.lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let mut c = l.split('\t');
            let group = c.next().expect("group").to_string();
            let name = c.next().expect("name").to_string();
            let kind = c.next().expect("kind");
            let payload = c.next().expect("payload");
            let expected: u32 = c.next().expect("expected").parse().expect("u32 expected");
            let bytes = match kind {
                "text" => payload.as_bytes().to_vec(),
                "bytes" => (0..payload.len() / 2)
                    .map(|i| u8::from_str_radix(&payload[i * 2..i * 2 + 2], 16).expect("hex"))
                    .collect(),
                "repeat" => {
                    let (hex, count) = payload.split_once(':').expect("HH:count");
                    let byte = u8::from_str_radix(hex, 16).expect("hex byte");
                    let n: usize = count.parse().expect("count");
                    vec![byte; n]
                }
                other => panic!("unknown kind: {other}"),
            };
            Vector {
                group,
                name,
                bytes,
                expected,
            }
        })
        .collect()
}

#[test]
fn anchors_match_canonical_published_vectors() {
    let anchors: Vec<_> = load().into_iter().filter(|v| v.group == "anchor").collect();
    assert!(anchors.len() >= 3, "expected >=3 published anchors");
    for v in &anchors {
        assert_eq!(fnv1a_32(&v.bytes), v.expected, "anchor {}", v.name);
    }
    let empty = anchors
        .iter()
        .find(|v| v.name == "empty")
        .expect("empty anchor");
    assert_eq!(empty.expected, 2166136261);
    // Empty input MUST return the offset basis (no bytes processed).
    assert_eq!(fnv1a_32(b""), 2166136261);
}

#[test]
fn adversarial_emoji_cyrillic_highbit_overflow_match_frozen_characterization() {
    let adv: Vec<_> = load()
        .into_iter()
        .filter(|v| v.group == "adversarial")
        .collect();
    for required in ["cyrillic", "emoji", "high-bit-bytes", "overflow-long-10000"] {
        assert!(
            adv.iter().any(|v| v.name == required),
            "missing required adversarial vector: {required}"
        );
    }
    for v in &adv {
        assert_eq!(fnv1a_32(&v.bytes), v.expected, "adversarial {}", v.name);
    }
}

#[test]
fn fuzz_500_vectors_match_frozen_characterization() {
    let fuzz: Vec<_> = load().into_iter().filter(|v| v.group == "fuzz").collect();
    assert!(
        fuzz.len() >= 500,
        "expected >=500 fuzz vectors, got {}",
        fuzz.len()
    );
    for v in &fuzz {
        assert_eq!(fnv1a_32(&v.bytes), v.expected, "fuzz {}", v.name);
    }
}

#[test]
fn live_property_determinism_and_no_overflow_panic() {
    // Deterministic LCG (reproducible): asserts f(x)==f(x) on random inputs and
    // that a huge key never panics (wrapping arithmetic, not debug overflow-panic).
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (state >> 33) as u8
    };
    for _ in 0..2000 {
        let len = 1 + (next() as usize % 64);
        let buf: Vec<u8> = (0..len).map(|_| next()).collect();
        assert_eq!(fnv1a_32(&buf), fnv1a_32(&buf), "deterministic");
    }
    let big = vec![0xABu8; 100_000];
    let _ = fnv1a_32(&big); // must not panic
}
