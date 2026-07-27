//! Byte contract for the private, dependency-free SHA-256 primitive.
//!
//! The production module is intentionally included by path: it remains private
//! to `labcolors-core`, while this integration target independently fixes its
//! byte-level contract.  No SHA implementation or third-party crate belongs in
//! this test slice.

#[path = "../src/sha256.rs"]
mod sha256;

use sha256::{Digest, Hasher, digest as sha256_digest};
use std::io::Write;
use std::process::{Command, Stdio};

fn assert_hex(input: &[u8], expected: &str) {
    let actual = sha256_digest(input).to_hex();
    assert_eq!(
        actual,
        expected,
        "SHA-256 mismatch for {} bytes",
        input.len()
    );
}

#[test]
fn fips_and_nist_known_answer_vectors() {
    // NIST/FIPS 180-4 examples plus the NIST empty-message vector.
    assert_hex(
        b"",
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    );
    assert_hex(
        b"abc",
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
    assert_hex(
        b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
    );

    let million_a = vec![b'a'; 1_000_000];
    assert_hex(
        &million_a,
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0",
    );
}

#[test]
fn padding_boundaries_match_external_known_answers() {
    // SHA-256 appends 0x80 and an eight-byte bit length.  These lengths straddle
    // the final-block and full-block transitions and catch off-by-one padding.
    let vectors = [
        (
            55,
            "9f4390f8d30c2dd92ec9f095b65e2b9ae9b0a925a5258e241c9f1e910f734318",
        ),
        (
            56,
            "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a",
        ),
        (
            63,
            "7d3e74a05d7db15bce4ad9ec0658ea98e3f06eeecf16b4c6fff2da457ddc2f34",
        ),
        (
            64,
            "ffe054fe7ae0cb6dc65c3af9b61d5209f439851db43d0ba5997337df154668eb",
        ),
        (
            65,
            "635361c48bb9eab14198e76ea8ab7f1a41685d6ad62aa9146d301d4f17eb0ae0",
        ),
    ];

    for (length, expected) in vectors {
        assert_hex(&vec![b'a'; length], expected);
    }
}

#[test]
fn streaming_preimages_are_byte_identical_without_a_second_payload_buffer() {
    let bytes: Vec<u8> = (0..=513)
        .map(|index| (index as u8).wrapping_mul(137).wrapping_add(29))
        .collect();
    let expected = sha256_digest(&bytes);

    for chunk_size in [1, 2, 7, 55, 56, 63, 64, 65, 127, 256, 1_024] {
        let mut hasher = Hasher::new();
        hasher.update(&[]);
        for chunk in bytes.chunks(chunk_size) {
            hasher.update(chunk);
        }
        hasher.update(&[]);
        assert_eq!(
            hasher.finalize(),
            expected,
            "streaming digest drifted at chunk size {chunk_size}"
        );
    }
}

#[test]
fn digest_is_exactly_typed_and_hex_is_canonical() {
    fn requires_traits<T: Copy + Eq + Ord>() {}
    fn requires_exact_bytes(_: &[u8; 32]) {}

    requires_traits::<Digest>();

    let value = sha256_digest(b"typed-digest");
    let copied = value;
    requires_exact_bytes(value.as_bytes());
    assert!(value == copied, "Digest must have value equality");

    let mut ordered = [sha256_digest(b"z"), sha256_digest(b"a"), value];
    ordered.sort();
    assert!(ordered.windows(2).all(|pair| pair[0] <= pair[1]));

    let hex = value.to_hex();
    assert_eq!(
        hex.len(),
        64,
        "one byte must encode to exactly two hex digits"
    );
    assert!(
        hex.bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "hex encoding must be canonical lowercase ASCII"
    );

    let mut reconstructed = String::with_capacity(64);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in value.as_bytes() {
        reconstructed.push(char::from(HEX[usize::from(byte >> 4)]));
        reconstructed.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    assert_eq!(hex, reconstructed, "hex must encode the exact typed bytes");
}

#[test]
fn deterministic_corpus_matches_python_hashlib() {
    // Python is already an explicit CI toolchain dependency for the independent
    // WCAG verifier.  `hashlib` is Python stdlib, so this adds neither a Rust
    // runtime dependency nor a second SHA implementation to production code.
    let mut corpus: Vec<Vec<u8>> = (0..=130)
        .map(|length| {
            (0..length)
                .map(|index| (index as u8).wrapping_mul(73).wrapping_add(length as u8))
                .collect()
        })
        .collect();
    corpus.extend([
        vec![0x00; 1_024],
        vec![0xff; 4_096],
        (0..100_000).map(|index| (index % 251) as u8).collect(),
        "opaque-client-id/Привет/🎨".as_bytes().to_vec(),
    ]);
    // Anti-vacuum for the bidirectional pipe protocol: the encoded input is
    // over 16 MiB and Python's digest output is over 2 MiB.  A mutation that
    // writes all stdin before draining stdout therefore blocks on finite OS
    // pipes instead of merely passing because the ordinary corpus is small.
    let pipe_stress_payload: Vec<u8> = (0..=u8::MAX).collect();
    corpus.extend((0..32 * 1_024).map(|_| pipe_stress_payload.clone()));

    let mut child = Command::new("python3")
        .args([
            "-c",
            concat!(
                "import hashlib, sys\n",
                "for line in sys.stdin:\n",
                " print(hashlib.sha256(bytes.fromhex(line.strip())).hexdigest())\n",
            ),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("python3 is part of the repository CI toolchain");

    let mut stdin = child.stdin.take().expect("piped Python stdin");
    let output = std::thread::scope(|scope| {
        let corpus_for_writer = &corpus;
        let writer = scope.spawn(move || {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            for bytes in corpus_for_writer {
                let mut line = Vec::with_capacity(bytes.len() * 2 + 1);
                for byte in bytes {
                    line.push(HEX[usize::from(byte >> 4)]);
                    line.push(HEX[usize::from(byte & 0x0f)]);
                }
                line.push(b'\n');
                // `panic` закрывает принадлежащий потоку канал до ожидания в
                // родителе, поэтому ошибка записи не оставит дочерний процесс.
                stdin.write_all(&line).expect("write corpus to hashlib");
            }
        });
        let output = child.wait_with_output().expect("wait for hashlib oracle");
        writer.join().expect("hashlib stdin writer panicked");
        output
    });
    assert!(
        output.status.success(),
        "hashlib oracle failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    let expected: Vec<_> = String::from_utf8(output.stdout)
        .expect("hashlib emits ASCII")
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(expected.len(), corpus.len(), "hashlib result count drift");

    for (index, (bytes, expected_hex)) in corpus.iter().zip(expected).enumerate() {
        assert_eq!(
            sha256_digest(bytes).to_hex(),
            expected_hex,
            "hashlib differential mismatch at corpus index {index} ({} bytes)",
            bytes.len(),
        );
    }
}
