/**
 * FNV-1a 32-bit hash of a raw byte sequence — JS mirror of the Rust core
 * primitive `labcolors_core::fnv1a_32`. Deterministic and byte-identical across
 * runtimes. Returns an unsigned 32-bit integer (0..=4294967295). The caller
 * encodes to bytes (e.g. `new TextEncoder().encode(str)` for UTF-8).
 */
export function fnv1a32(bytes: Uint8Array | number[]): number;
