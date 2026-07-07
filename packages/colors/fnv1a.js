// Portable non-cryptographic hash — JS mirror of the Rust core primitive
// `labcolors_core::fnv1a_32` (crates/labcolors-core/src/hash.rs).
//
// Agnostic: knows nothing about hues, accents, or roles — bytes in, u32 out.
// The deterministic auto-accent selection that will sit on top of this lives in
// the consumer (labui), not here.
//
// Provenance: Fowler–Noll–Vo, variant 1a, 32-bit. offset_basis = 2166136261
// (0x811c9dc5), prime = 16777619 (0x01000193). Canonical spec + published test
// vectors: http://www.isthe.com/chongo/tech/comp/fnv/. Byte-identical to the
// Rust impl on every shared vector (see the JS<->Rust differential test
// packages/colors/test/fnv1a-differential.test.mjs).

const FNV1A_32_OFFSET_BASIS = 2166136261; // 0x811c9dc5
const FNV1A_32_PRIME = 16777619; // 0x01000193

/**
 * FNV-1a 32-bit hash of an arbitrary byte sequence.
 *
 * Deterministic and portable: returns the same unsigned 32-bit integer as the
 * Rust `fnv1a_32` on identical input. The caller encodes to bytes (e.g.
 * `new TextEncoder().encode(str)` for UTF-8) — the hash is byte-oriented and
 * knows no string semantics.
 *
 * Overflow discipline: JS multiplication overflows the 2^53 safe-integer range
 * for the 32×32-bit product, so we split the multiply into 16-bit halves and
 * mask with `>>> 0` at each step to stay byte-identical to Rust's wrapping u32
 * arithmetic. The result is always an unsigned 32-bit integer (0..=0xffffffff).
 *
 * @param {Uint8Array | number[]} bytes raw bytes to hash
 * @returns {number} unsigned 32-bit hash (0..=4294967295)
 */
export function fnv1a32(bytes) {
  let hash = FNV1A_32_OFFSET_BASIS >>> 0;
  for (let i = 0; i < bytes.length; i++) {
    hash ^= bytes[i] & 0xff;
    // 32-bit multiply without precision loss: (hash * prime) mod 2^32.
    // Split hash into low/high 16 bits so every partial product stays < 2^53.
    const lo = (hash & 0xffff) * FNV1A_32_PRIME;
    const hi = ((hash >>> 16) * FNV1A_32_PRIME) & 0xffff;
    hash = ((((hi << 16) >>> 0) + lo) & 0xffffffff) >>> 0;
  }
  return hash >>> 0;
}
