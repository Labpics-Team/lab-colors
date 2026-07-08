// Differential + anchor test for the FNV-1a 32-bit JS mirror.
//
// One source of truth for vectors: crates/labcolors-core/tests/data/fnv1a-vectors.txt
// (LF-pinned TSV), shared byte-for-byte with the Rust integration test
// (crates/labcolors-core/tests/fnv1a_differential.rs). Both sides recompute
// every vector and assert equality against the committed expected (unsigned
// decimal u32). Green on both = byte-identical JS==Rust output on every vector:
// empty string, Cyrillic, emoji, high-bit bytes, an overflow-length key, and a
// 500-vector randomized fuzz corpus.
//
// anchors carry the CANONICAL published FNV-1a values (external ground truth,
// http://www.isthe.com/chongo/tech/comp/fnv/) so correctness is grounded in the
// spec, not self-blessed. `text` vectors are stored as literal strings so this
// runtime exercises its OWN UTF-8 encoding path (the real cross-runtime risk).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { fnv1a32 } from '../fnv1a.js';

const here = dirname(fileURLToPath(import.meta.url));
const raw = readFileSync(
  join(here, '../../../crates/labcolors-core/tests/data/fnv1a-vectors.txt'),
  'utf8',
);

const enc = new TextEncoder();
function bytesOf(kind, payload) {
  if (kind === 'text') return enc.encode(payload);
  if (kind === 'bytes') {
    const out = new Uint8Array(payload.length / 2);
    for (let i = 0; i < out.length; i++) out[i] = parseInt(payload.slice(i * 2, i * 2 + 2), 16);
    return out;
  }
  if (kind === 'repeat') {
    const [hex, count] = payload.split(':');
    return new Uint8Array(Number(count)).fill(parseInt(hex, 16));
  }
  throw new Error(`unknown kind: ${kind}`);
}

const vectors = raw
  .split(/\r?\n/)
  .filter((l) => l.length > 0 && !l.startsWith('#'))
  .map((l) => {
    const [group, name, kind, payload, expected] = l.split('\t');
    return { group, name, kind, payload: payload ?? '', expected: Number(expected) };
  });
const byGroup = (g) => vectors.filter((v) => v.group === g);

test('anchors: mirror matches canonical published FNV-1a vectors', () => {
  const anchors = byGroup('anchor');
  assert.ok(anchors.length >= 3, 'expected >=3 published anchors');
  for (const v of anchors)
    assert.strictEqual(fnv1a32(bytesOf(v.kind, v.payload)), v.expected, `anchor ${v.name}`);
  const empty = anchors.find((v) => v.name === 'empty');
  assert.strictEqual(empty.expected, 2166136261, 'empty == offset basis');
});

test('adversarial: emoji / cyrillic / high-bit / overflow all match oracle', () => {
  const adv = byGroup('adversarial');
  const names = new Set(adv.map((v) => v.name));
  for (const req of ['cyrillic', 'emoji', 'high-bit-bytes', 'overflow-long-10000'])
    assert.ok(names.has(req), `missing required adversarial vector: ${req}`);
  for (const v of adv)
    assert.strictEqual(fnv1a32(bytesOf(v.kind, v.payload)), v.expected, `adversarial ${v.name}`);
});

test('fuzz: >=500 frozen random vectors match oracle (cross-runtime differential)', () => {
  const fuzz = byGroup('fuzz');
  assert.ok(fuzz.length >= 500, `expected >=500 fuzz vectors, got ${fuzz.length}`);
  for (const v of fuzz)
    assert.strictEqual(fnv1a32(bytesOf(v.kind, v.payload)), v.expected, `fuzz ${v.name}`);
});

test('live property: unsigned u32 range + determinism on random input', () => {
  for (let i = 0; i < 2000; i++) {
    const len = 1 + Math.floor(Math.random() * 40);
    const arr = new Uint8Array(len);
    for (let j = 0; j < len; j++) arr[j] = Math.floor(Math.random() * 256);
    const a = fnv1a32(arr);
    assert.strictEqual(a, fnv1a32(arr), 'deterministic');
    assert.ok(Number.isInteger(a) && a >= 0 && a <= 0xffffffff, `u32 range: ${a}`);
  }
});
