#!/usr/bin/env python3
"""Independent controls for the narrow WCAG22 Rust source-route binding."""

from __future__ import annotations

import hashlib
import json
import re
import runpy
import struct
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
CORE_SOURCE = REPO_ROOT / "crates/labcolors-core/src"
CRATE_ROOT = CORE_SOURCE / "lib.rs"
SRGB8_PARENT = CORE_SOURCE / "srgb8.rs"
KERNEL = CORE_SOURCE / "wcag22/kernel.rs"
PROOF = (
    REPO_ROOT
    / "crates/labcolors-core/contracts/wcag22-srgb8-q55-proof-v1.json"
)
VERIFIER = REPO_ROOT / "scripts/verify_wcag22_q55.py"

SCHEMA_VERSION = 1
LAW = "wcag22-rust-semantic-dependency-cone-v1"
DOMAIN = b"labcolors.wcag22-source-binding"
ROOT_BEGIN = b"// BEGIN WCAG22_SOURCE_ROUTES_V1"
ROOT_END = b"// END WCAG22_SOURCE_ROUTES_V1"
ROOT_REGION = b"""// BEGIN WCAG22_SOURCE_ROUTES_V1
const _: () = (); // First-item proof anchor; moving it fails verify_wcag22_q55.py.
pub mod numerics;
pub(crate) mod srgb8;
pub mod wcag22;
#[doc(hidden)]
pub mod wcag22_evidence;
// END WCAG22_SOURCE_ROUTES_V1"""
PARSER_BEGIN = b"// BEGIN WCAG22_PARSER_CAPSULE_V1"
PARSER_END = b"// END WCAG22_PARSER_CAPSULE_V1"
PARSER_REGION = b"""// BEGIN WCAG22_PARSER_CAPSULE_V1
const _: () = (); // First-item parser proof anchor; moving it fails verify_wcag22_q55.py.
/// Parse optional-`#` `RRGGBB` into exact encoded-sRGB8 bytes shared by colour math and proofs.
///
/// Public APIs choose their own transport strictness before calling this SSOT.
/// ASCII is checked before byte slicing, so arbitrary public Unicode input
/// returns `Err` instead of panicking at a non-character boundary.
pub(crate) fn hex_bytes(hex: &str) -> Result<[u8; 3], String> {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if hex.len() != 6 || !hex.is_ascii() {
        return Err(format!("expected #RRGGBB, got #{hex}"));
    }
    let parse = |value: &str| u8::from_str_radix(value, 16).map_err(|error| error.to_string());
    Ok([parse(&hex[0..2])?, parse(&hex[2..4])?, parse(&hex[4..6])?])
}
// END WCAG22_PARSER_CAPSULE_V1"""


def length_prefixed(value: bytes) -> bytes:
    return struct.pack("<I", len(value)) + value


def extract_region(source: bytes, begin: bytes, end: bytes) -> bytes:
    begins = list(re.finditer(rb"(?m)^" + re.escape(begin) + rb"$", source))
    ends = list(re.finditer(rb"(?m)^" + re.escape(end) + rb"$", source))
    if len(begins) != 1 or len(ends) != 1:
        raise AssertionError("source-binding markers must occur exactly once")
    start = begins[0].start()
    if start != 0:
        raise AssertionError("source-binding region must be the first source item")
    stop = ends[0].end()
    return source[start:stop]


def route_digest(root_region: bytes, parser_region: bytes) -> str:
    records = (
        (
            b"crates/labcolors-core/Cargo.toml",
            b"cargo-lib-target-v1",
            b"crates/labcolors-core/src/lib.rs",
        ),
        (
            b"crates/labcolors-core/src/lib.rs",
            b"wcag22-source-routes-v1",
            root_region,
        ),
        (
            b"crates/labcolors-core/src/srgb8.rs",
            b"wcag22-parser-capsule-v1",
            parser_region,
        ),
    )
    preimage = bytearray(length_prefixed(DOMAIN))
    preimage.extend(struct.pack("<I", SCHEMA_VERSION))
    preimage.extend(length_prefixed(LAW.encode("utf-8")))
    preimage.extend(struct.pack("<I", len(records)))
    for path, region_id, region in records:
        preimage.extend(length_prefixed(path))
        preimage.extend(length_prefixed(region_id))
        preimage.extend(length_prefixed(region))
    return hashlib.sha256(bytes(preimage)).hexdigest()


def live_route_digest(root_source: bytes, parser_source: bytes) -> str:
    root = extract_region(root_source, ROOT_BEGIN, ROOT_END)
    parser = extract_region(parser_source, PARSER_BEGIN, PARSER_END)
    if root != ROOT_REGION or parser != PARSER_REGION:
        raise AssertionError("canonical source-binding region drifted")
    return route_digest(root, parser)


class SourceBindingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.root_source = CRATE_ROOT.read_bytes()
        cls.parser_parent_source = SRGB8_PARENT.read_bytes()
        cls.proof = json.loads(PROOF.read_text(encoding="utf-8-sig"))

    def test_committed_proof_binds_the_exact_live_capsules(self) -> None:
        self.assertEqual(self.proof["schema_version"], 2)
        self.assertEqual(self.proof["source_binding_schema_version"], SCHEMA_VERSION)
        self.assertEqual(self.proof["source_binding_law"], LAW)
        self.assertEqual(
            self.proof["source_route_sha256"],
            live_route_digest(self.root_source, self.parser_parent_source),
        )
        self.assertNotIn("crate_lib_source_sha256", self.proof)
        verifier = VERIFIER.read_text(encoding="utf-8-sig")
        self.assertNotIn("CRATE_LIB_SOURCE", verifier)
        self.assertNotIn("crate_lib_source_sha256", verifier)

    def test_cargo_metadata_rejects_redirected_crate_root(self) -> None:
        verifier = runpy.run_path(str(VERIFIER), run_name="wcag22_verifier_test")
        verify_target = verifier["verify_canonical_crate_target"]
        self.assertEqual(
            verify_target(),
            b"crates/labcolors-core/src/lib.rs",
        )
        with tempfile.TemporaryDirectory(prefix="labcolors-route-target-") as temp:
            root = Path(temp)
            source = root / "src"
            source.mkdir()
            manifest = root / "Cargo.toml"
            manifest.write_text(
                "[package]\n"
                'name = "redirected-root"\n'
                'version = "0.0.0"\n'
                'edition = "2024"\n'
                "[lib]\n"
                'path = "src/alternate.rs"\n',
                encoding="utf-8",
            )
            expected = source / "lib.rs"
            expected.write_text("pub fn canonical() {}\n", encoding="utf-8")
            (source / "alternate.rs").write_text(
                "pub fn redirected() {}\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(AssertionError, "crate root redirect"):
                verify_target(
                    manifest_path=manifest,
                    expected_source=expected,
                    logical_source=b"src/lib.rs",
                )

    def test_every_bound_route_mutation_is_rejected_fail_closed(self) -> None:
        root_mutations = (
            self.root_source.replace(b"pub(crate) mod srgb8;", b"pub mod srgb8;", 1),
            self.root_source.replace(b"pub mod wcag22;", b"pub mod other;", 1),
            self.root_source.replace(
                b"pub mod wcag22;",
                b"#[cfg(any())]\npub mod wcag22;",
                1,
            ),
            self.root_source.replace(
                b"pub mod wcag22;",
                b'#[path = "alternate.rs"]\npub mod wcag22;',
                1,
            ),
            self.root_source.replace(
                b"pub mod wcag22_evidence;",
                b"pub mod alternate_evidence;",
                1,
            ),
            self.root_source.replace(b"pub mod numerics;", b"pub mod alternate;", 1),
        )
        parser_mutations = (
            self.parser_parent_source.replace(
                b"strip_prefix('#').unwrap_or(hex)",
                b"trim_start_matches('#')",
                1,
            ),
            self.parser_parent_source.replace(
                b"u8::from_str_radix(value, 16)",
                b"alternate_parser(value)",
                1,
            ),
        )
        for root in root_mutations:
            self.assertNotEqual(root, self.root_source)
            with self.assertRaises(AssertionError):
                live_route_digest(root, self.parser_parent_source)
        for parser in parser_mutations:
            self.assertNotEqual(parser, self.parser_parent_source)
            with self.assertRaises(AssertionError):
                live_route_digest(self.root_source, parser)

    def test_missing_duplicate_and_partial_line_markers_are_rejected(self) -> None:
        for source in (
            self.root_source.replace(ROOT_BEGIN, b"", 1),
            self.root_source + b"\n" + ROOT_BEGIN + b"\n" + ROOT_END,
            self.root_source.replace(ROOT_BEGIN, b"prefix " + ROOT_BEGIN, 1),
        ):
            with self.subTest(source=source):
                with self.assertRaises(AssertionError):
                    extract_region(source, ROOT_BEGIN, ROOT_END)

    def test_unrelated_api_outside_capsules_does_not_change_binding(self) -> None:
        baseline = live_route_digest(self.root_source, self.parser_parent_source)
        root_additions = (
            b"\npub mod unrelated_source_binding_control;\n",
            b"\n// unrelated crate-root comment\n",
        )
        for addition in root_additions:
            self.assertEqual(
                live_route_digest(
                    self.root_source + addition,
                    self.parser_parent_source,
                ),
                baseline,
            )
        self.assertEqual(
            live_route_digest(
                self.root_source + b"\npub use srgb8::Srgb8;\n",
                self.parser_parent_source + b"\npub struct Srgb8([u8; 3]);\n",
            ),
            baseline,
        )
        self.assertEqual(
            live_route_digest(
                self.root_source,
                self.parser_parent_source
                + b"\n#[cfg(test)] mod unrelated_route_tests {}\n",
            ),
            baseline,
        )

    def test_inert_first_item_blocks_outer_attribute_attachment(self) -> None:
        for region in (ROOT_REGION, PARSER_REGION):
            lines = region.splitlines()
            self.assertTrue(lines[1].startswith(b"const _: () = ();"))
            self.assertIn(b"proof anchor", lines[1])

    def test_parser_capsule_is_exact_production_only_code(self) -> None:
        source = extract_region(self.parser_parent_source, PARSER_BEGIN, PARSER_END)
        self.assertFalse(SRGB8_PARENT.is_symlink())
        self.assertEqual(
            hashlib.sha256(source).hexdigest(),
            self.proof["parser_source_sha256"],
        )
        clean = source.decode("utf-8")
        self.assertEqual(re.findall(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(", clean), ["hex_bytes"])
        for fragment in (
            "strip_prefix('#').unwrap_or(hex)",
            "hex.len() != 6 || !hex.is_ascii()",
            "u8::from_str_radix(value, 16)",
            "parse(&hex[0..2])?",
            "parse(&hex[2..4])?",
            "parse(&hex[4..6])?",
        ):
            self.assertIn(fragment, clean)
        self.assertNotIn("trim_start_matches", clean)
        self.assertNotIn("unwrap_or(0)", clean)

    def test_kernel_call_redirects_are_rejected_by_exact_leaf_binding(self) -> None:
        verifier = runpy.run_path(str(VERIFIER), run_name="wcag22_verifier_test")
        verify_kernel = verifier["verify_production_kernel"]
        source = KERNEL.read_text(encoding="utf-8-sig")
        redirects = (
            source.replace(
                "crate::srgb8::hex_bytes(value)",
                "crate::alternate::hex_bytes(value)",
                1,
            ),
            source.replace(
                "mint_wcag22_evidence()",
                "alternate_evidence()",
                1,
            ),
        )
        for redirected in redirects:
            self.assertNotEqual(redirected, source)
        with tempfile.TemporaryDirectory(prefix="labcolors-kernel-route-") as temp:
            original = verify_kernel.__globals__["KERNEL_SOURCE"]
            try:
                for index, redirected in enumerate(redirects):
                    path = Path(temp) / f"kernel-{index}.rs"
                    path.write_text(redirected, encoding="utf-8")
                    verify_kernel.__globals__["KERNEL_SOURCE"] = path
                    with self.assertRaisesRegex(AssertionError, "kernel drifted"):
                        verify_kernel()
            finally:
                verify_kernel.__globals__["KERNEL_SOURCE"] = original


if __name__ == "__main__":
    unittest.main(verbosity=2)
