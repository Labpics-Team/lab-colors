#!/usr/bin/env python3
"""Hostile contract for the comparator wire bundle (V5b2d-2b).

Verification lanes replay under a BUILD-derived engine comparator, never
under the fixture lane comparator: ``load_lane_v1`` refuses any lane whose
manifest binds a different comparator identity.  The lane runner therefore
needs a wire form for an arbitrary admitted comparator — the canonical
``ComparatorManifestV2`` encoding plus the exact content bytes its ten
addresses name.  The bundle is a closed wire surface: it round-trips the
admitted comparator identity byte-for-byte, and any corruption of the
manifest or of a single content coordinate refuses to admit.
"""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

PROOF = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PROOF))

import corpus  # noqa: E402
import corpus_assembly  # noqa: E402
import corpus_lane  # noqa: E402
import region_proof_protocol as protocol  # noqa: E402

FIXTURE_JOB_V1 = PROOF / "fixtures" / "proof-job-v1.bin"
ARB_KIND = protocol.ComparatorKindV1.ARB


def _distinct_contents(tag: str) -> tuple[bytes, ...]:
    return tuple(
        f"{tag}-coordinate-{index}".encode("ascii") for index in range(10)
    )


def _contents_map(contents: tuple[bytes, ...]) -> dict[bytes, bytes]:
    return {hashlib.sha256(content).digest(): content for content in contents}


def _resolved(tag: str) -> protocol.ContentResolvedComparatorManifestV2:
    contents = _distinct_contents(tag)
    return protocol.ContentResolvedComparatorManifestV2.admit(
        protocol.ComparatorManifestV2(
            ARB_KIND,
            *(hashlib.sha256(content).digest() for content in contents),
        ),
        _contents_map(contents).get,
    )


def _write_bundle(
    out: Path,
    manifest: protocol.ComparatorManifestV2,
    contents: dict[bytes, bytes],
) -> None:
    corpus_lane.write_comparator_bundle_v1(manifest, contents, out)


class ComparatorBundleWireTests(unittest.TestCase):
    def test_round_trip_preserves_admitted_identity(self) -> None:
        comparator = _resolved("wire-round-trip")
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "bundle"
            corpus_lane.write_comparator_bundle_v1(
                comparator.manifest,
                _contents_map(_distinct_contents("wire-round-trip")),
                out,
            )
            loaded = corpus_lane.load_comparator_bundle_v1(out)
            self.assertIs(type(loaded), protocol.ContentResolvedComparatorManifestV2)
            self.assertEqual(loaded.identity, comparator.identity)
            self.assertEqual(loaded.manifest, comparator.manifest)

    def test_bundle_layout_is_manifest_plus_one_file_per_address(self) -> None:
        comparator = _resolved("wire-layout")
        contents = _contents_map(_distinct_contents("wire-layout"))
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "bundle"
            corpus_lane.write_comparator_bundle_v1(
                comparator.manifest, contents, out
            )
            written = sorted(
                path.relative_to(out).as_posix() for path in out.rglob("*") if path.is_file()
            )
            expected = sorted(
                ["comparator-manifest-v2.bin"]
                + [
                    f"content/{address.hex()}"
                    for address in contents
                ]
            )
            self.assertEqual(written, expected)
            self.assertEqual(
                (out / "comparator-manifest-v2.bin").read_bytes(),
                comparator.manifest.encode(),
            )

    def test_write_requires_bytes_for_every_address(self) -> None:
        comparator = _resolved("wire-incomplete")
        contents = _contents_map(_distinct_contents("wire-incomplete"))
        addresses = tuple(contents)
        del contents[addresses[3]]
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(protocol.ProtocolErrorV1):
                corpus_lane.write_comparator_bundle_v1(
                    comparator.manifest, contents, Path(tmp) / "bundle"
                )

    def test_write_rejects_foreign_input(self) -> None:
        comparator = _resolved("wire-foreign")
        contents = _contents_map(_distinct_contents("wire-foreign"))
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "bundle"
            with self.assertRaises(protocol.ProtocolErrorV1):
                corpus_lane.write_comparator_bundle_v1(object(), contents, out)
            with self.assertRaises(protocol.ProtocolErrorV1):
                corpus_lane.write_comparator_bundle_v1(
                    comparator.manifest, object(), out
                )


class HostileBundleAdmissionTests(unittest.TestCase):
    def _bundle(self, tmp: str) -> tuple[Path, protocol.ComparatorManifestV2, dict[bytes, bytes]]:
        comparator = _resolved("wire-hostile")
        out = Path(tmp) / "bundle"
        corpus_lane.write_comparator_bundle_v1(
            comparator.manifest,
            _contents_map(_distinct_contents("wire-hostile")),
            out,
        )
        return out, comparator.manifest, _contents_map(_distinct_contents("wire-hostile"))

    def test_missing_bundle_directory_refuses(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(protocol.ProtocolErrorV1):
                corpus_lane.load_comparator_bundle_v1(Path(tmp) / "absent")

    def test_corrupted_manifest_bytes_refuse(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out, _, _ = self._bundle(tmp)
            raw = bytearray((out / "comparator-manifest-v2.bin").read_bytes())
            raw[-1] ^= 0xFF
            (out / "comparator-manifest-v2.bin").write_bytes(bytes(raw))
            with self.assertRaises(protocol.ProtocolErrorV1):
                corpus_lane.load_comparator_bundle_v1(out)

    def test_missing_content_file_refuses(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out, _, _ = self._bundle(tmp)
            victim = next(iter(sorted((out / "content").iterdir())))
            victim.unlink()
            with self.assertRaises(protocol.ProtocolErrorV1):
                corpus_lane.load_comparator_bundle_v1(out)

    def test_tampered_content_bytes_refuse(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out, _, _ = self._bundle(tmp)
            victim = next(iter(sorted((out / "content").iterdir())))
            victim.write_bytes(victim.read_bytes() + b"tamper")
            with self.assertRaises(protocol.ProtocolErrorV1):
                corpus_lane.load_comparator_bundle_v1(out)

    def test_foreign_file_inside_content_refuses(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out, _, _ = self._bundle(tmp)
            (out / "content" / "not-a-digest.bin").write_bytes(b"surprise")
            with self.assertRaises(protocol.ProtocolErrorV1):
                corpus_lane.load_comparator_bundle_v1(out)

    def test_foreign_directory_argument_refuses(self) -> None:
        with self.assertRaises(protocol.ProtocolErrorV1):
            corpus_lane.load_comparator_bundle_v1(object())


class LaneRunnerComparatorFlagsTests(unittest.TestCase):
    """The lane runner must accept an arbitrary job and comparator bundle."""

    def test_runner_replays_under_the_bundle_comparator(self) -> None:
        comparator = _resolved("runner-flags")
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            bundle = root / "bundle"
            corpus_lane.write_comparator_bundle_v1(
                comparator.manifest,
                _contents_map(_distinct_contents("runner-flags")),
                bundle,
            )
            lane_out = root / "lane"
            result = subprocess.run(
                [
                    sys.executable,
                    str(PROOF / "corpus_lane.py"),
                    "--job",
                    str(FIXTURE_JOB_V1),
                    "--comparator-bundle",
                    str(bundle),
                    "--window-start",
                    "0",
                    "--window-points",
                    "64",
                    "--shard-points",
                    "32",
                    "--out",
                    str(lane_out),
                ],
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            manifest = json.loads((lane_out / "lane-manifest.json").read_bytes())
            self.assertEqual(manifest["comparator_identity"], comparator.identity.hex())
            job = corpus.full_domain_job_v1(
                protocol.ProofJobV1.parse(FIXTURE_JOB_V1.read_bytes())
            )
            lane = corpus_assembly.load_lane_v1(lane_out, job, comparator)
            self.assertIs(type(lane), corpus_assembly.AdmittedLaneV1)

    def test_corrupted_bundle_exits_64(self) -> None:
        comparator = _resolved("runner-corrupt")
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            bundle = root / "bundle"
            corpus_lane.write_comparator_bundle_v1(
                comparator.manifest,
                _contents_map(_distinct_contents("runner-corrupt")),
                bundle,
            )
            victim = next(iter(sorted((bundle / "content").iterdir())))
            victim.write_bytes(victim.read_bytes() + b"tamper")
            result = subprocess.run(
                [
                    sys.executable,
                    str(PROOF / "corpus_lane.py"),
                    "--job",
                    str(FIXTURE_JOB_V1),
                    "--comparator-bundle",
                    str(bundle),
                    "--window-start",
                    "0",
                    "--window-points",
                    "64",
                    "--shard-points",
                    "32",
                    "--out",
                    str(root / "lane"),
                ],
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 64)


if __name__ == "__main__":
    unittest.main()
