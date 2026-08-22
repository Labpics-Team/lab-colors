#!/usr/bin/env python3
"""Contract tests for the MPFI source-owned sealed BUILD input."""

from __future__ import annotations

import hashlib
import io
import sys
import tarfile
import unittest
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "tests"))

import provenance  # noqa: E402
from build import input as build_input  # noqa: E402
from mpfi import build as mpfi_build  # noqa: E402
from mpfi.evaluator import formula  # noqa: E402
from test_mpfi_input import _admitted_closure  # noqa: E402


def _generated_formula() -> bytes:
    source = (ROOT.parents[2] / "crates/labcolors-core/contracts/contextual-region-formula-v1.lcir").read_bytes()
    return formula.emit(formula.parse(source))


def _workspace_sources() -> mpfi_build.AdmittedMpfiBuildSourcesV1:
    files = tuple(
        mpfi_build.MpfiBuildSourceFileV1(
            path,
            mode,
            (ROOT.parents[2] / path).read_bytes(),
        )
        for path, mode in mpfi_build.REQUIRED_WORKSPACE_MODES_V1
    )
    return mpfi_build.admit_mpfi_build_sources_v1(files)


def _limits_for_bundle(
    source_lock: provenance.MpfiSourceLockV1,
    admitted: provenance.AdmittedMpfiSourcesV1,
    sources: mpfi_build.AdmittedMpfiBuildSourcesV1,
    generated: bytes,
) -> build_input.CanonicalInputLimitsV1:
    replayed = provenance.replay_admitted_source_closure_v1(source_lock, admitted)
    entries = list(
        mpfi_build.canonical_input_entries_v1(replayed, sources, generated)
    )
    directories = {
        "/".join(path.split("/")[:length])
        for path, _mode, _contents in entries
        for length in range(1, len(path.split("/")))
    }
    return build_input.CanonicalInputLimitsV1(
        len(entries) + len(directories),
        max(len(contents) for _path, _mode, contents in entries),
        sum(len(contents) for _path, _mode, contents in entries),
    )


def _members(value: build_input.SealedInputV1) -> tuple[str, ...]:
    with tarfile.open(fileobj=io.BytesIO(value.contents), mode="r:") as archive:
        return tuple(member.name for member in archive.getmembers())


class MpfiBuildInputTests(unittest.TestCase):
    def test_sealed_bundle_contains_source_input_generated_formula_and_workspace(self) -> None:
        source_lock, admitted, _entries = _admitted_closure()
        sources = _workspace_sources()
        generated = _generated_formula()
        limits = _limits_for_bundle(source_lock, admitted, sources, generated)

        first = mpfi_build.seal_mpfi_build_input_v1(
            source_lock,
            admitted,
            sources,
            generated,
            limits,
        )
        second = mpfi_build.seal_mpfi_build_input_v1(
            source_lock,
            admitted,
            sources,
            generated,
            limits,
        )
        self.assertEqual(first, second)
        self.assertTrue(build_input.sealed_input_is_intact_v1(first))
        self.assertTrue(
            mpfi_build.mpfi_build_input_is_bound_v1(
                source_lock,
                admitted,
                sources,
                generated,
                limits,
                first,
            )
        )
        members = _members(first)
        self.assertIn("inputs/formula.generated.c", members)
        self.assertIn("workspace/proof/region/v1/mpfi/build.sh", members)
        self.assertIn("workspace/proof/region/v1/mpfi/evaluator/wire.c", members)
        self.assertIn("inputs/sources/gmp/value", members)

    def test_formula_and_workspace_mutations_are_red_before_transport(self) -> None:
        source_lock, admitted, _entries = _admitted_closure()
        sources = _workspace_sources()
        generated = _generated_formula()
        limits = _limits_for_bundle(source_lock, admitted, sources, generated)
        sealed = mpfi_build.seal_mpfi_build_input_v1(
            source_lock,
            admitted,
            sources,
            generated,
            limits,
        )

        with self.assertRaises(ValueError):
            mpfi_build.seal_mpfi_build_input_v1(
                source_lock,
                admitted,
                sources,
                generated[:-1] + bytes((generated[-1] ^ 1,)),
                limits,
            )
        changed = list(sources.files)
        item = changed[0]
        changed[0] = mpfi_build.MpfiBuildSourceFileV1(
            item.path,
            item.mode,
            item.contents + b"\n",
        )
        foreign = mpfi_build.AdmittedMpfiBuildSourcesV1(
            tuple(changed),
            hashlib.sha256(b"foreign").digest(),
        )
        self.assertFalse(
            mpfi_build.mpfi_build_input_is_bound_v1(
                source_lock,
                admitted,
                foreign,
                generated,
                limits,
                sealed,
            )
        )

    def test_retained_workspace_identity_is_replayed_not_trusted(self) -> None:
        source_lock, admitted, _entries = _admitted_closure()
        sources = _workspace_sources()
        generated = _generated_formula()
        limits = _limits_for_bundle(source_lock, admitted, sources, generated)
        poisoned = mpfi_build.AdmittedMpfiBuildSourcesV1(
            sources.files,
            hashlib.sha256(b"poison").digest(),
        )
        with self.assertRaises(ValueError):
            mpfi_build.seal_mpfi_build_input_v1(
                source_lock,
                admitted,
                poisoned,
                generated,
                limits,
            )

    def test_policy_is_pinned_to_the_clang_19_linux_amd64_manifest(self) -> None:
        policy = mpfi_build.MPFI_BUILD_TRANSPORT_POLICY_V1
        self.assertEqual(policy.platform, "linux/amd64")
        self.assertEqual(
            policy.image_reference,
            mpfi_build.MPFI_BUILD_IMAGE_REFERENCE_V1,
        )
        self.assertEqual(
            policy.image_reference,
            "silkeh/clang@sha256:"
            "f1d693e7af5ee954370e1f3605830d8cabc05f9731226fc99aa5e26127797c11",
        )
        self.assertIn("/build/work/mpfi-evaluator-v1", policy.bootstrap)
        self.assertIn("proof/region/v1/mpfi/build.sh", policy.bootstrap)

    def test_canonical_input_limits_reject_an_undersized_member_bound(self) -> None:
        source_lock, admitted, _entries = _admitted_closure()
        sources = _workspace_sources()
        generated = _generated_formula()
        limits = _limits_for_bundle(source_lock, admitted, sources, generated)
        undersized = build_input.CanonicalInputLimitsV1(
            limits.max_members - 1,
            limits.max_file_bytes,
            limits.max_payload_bytes,
        )
        with self.assertRaises(ValueError):
            mpfi_build.seal_mpfi_build_input_v1(
                source_lock,
                admitted,
                sources,
                generated,
                undersized,
            )

    def test_invalid_limits_are_rejected_before_source_replay(self) -> None:
        source_lock, admitted, _entries = _admitted_closure()
        sources = _workspace_sources()
        generated = _generated_formula()
        with mock.patch.object(
            provenance,
            "replay_admitted_source_closure_v1",
            side_effect=AssertionError("source replay happened before limit admission"),
        ):
            with self.assertRaises(TypeError):
                mpfi_build.seal_mpfi_build_input_v1(
                    source_lock,
                    admitted,
                    sources,
                    generated,
                    object(),
                )

    def test_snapshot_bound_check_does_not_replay_an_owned_closure(self) -> None:
        source_lock, admitted, _entries = _admitted_closure()
        snapshot = provenance.replay_admitted_source_closure_v1(source_lock, admitted)
        sources = _workspace_sources()
        generated = _generated_formula()
        limits = _limits_for_bundle(source_lock, admitted, sources, generated)
        sealed = mpfi_build.seal_mpfi_build_input_from_snapshot_v1(
            snapshot,
            sources,
            generated,
            limits,
        )
        with mock.patch.object(
            provenance,
            "replay_admitted_source_closure_v1",
            side_effect=AssertionError("snapshot path replayed the closure"),
        ):
            self.assertTrue(
                mpfi_build.mpfi_build_input_is_bound_from_snapshot_v1(
                    snapshot,
                    sources,
                    generated,
                    limits,
                    sealed,
                )
            )


if __name__ == "__main__":
    unittest.main(verbosity=2)
