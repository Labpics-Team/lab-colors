#!/usr/bin/env python3
"""The MPFI engine's source identity must be answerable before its build.

A dual proof needs both halves.  A coverage pre-check can show cheaply that a
set of lanes carries exactly two distinct comparator source identities, but not
that those two belong to the engines this job is about to build.  Arb answers
that from admitted inputs alone; without the same answer for MPFI the binding
still costs a native Docker build to discover, so the pre-check can only bind
one engine of the pair.

What these tests establish, and nothing beyond it:

1. the pre-check and the build path agree because they physically run one
   derivation — disabling it takes both down;
2. the answer is invariant to every build observation.

They do NOT establish that the derivation computes the *correct* coordinates.
No independent anchor for that exists in this repository: no pinned expected
comparator source identity, and no second implementation to differ against.
The Arb half carries exactly the same limitation.
"""

from __future__ import annotations

import sys
import unittest
from dataclasses import fields as dataclass_fields
from functools import cache
from pathlib import Path
from unittest import mock

PROOF = Path(__file__).resolve().parents[2]
TESTS = PROOF / "tests"
ARB_TESTS = PROOF / "arb/tests"
MPFI_TESTS = Path(__file__).resolve().parent
sys.path[:0] = [str(PROOF), str(TESTS), str(ARB_TESTS), str(MPFI_TESTS)]

import executor  # noqa: E402
import region_proof_protocol as protocol  # noqa: E402
from build import transport as build_transport  # noqa: E402
from mpfi import build as mpfi_build  # noqa: E402
from mpfi import receipt, runtime as mpfi_runtime  # noqa: E402
from arb import pipeline as arb_pipeline  # noqa: E402
from test_mpfi_build import (  # noqa: E402
    _generated_formula,
    _limits_for_bundle,
    _workspace_sources,
)
from test_mpfi_input import _admitted_closure  # noqa: E402
from test_pipeline import (  # noqa: E402
    _BuildBackend,
    _digest,
    _docker_capability,
    _job,
    _request as _arb_request,
    _runtime_binding as _arb_runtime_binding,
    _static_elf,
)
from test_receipt import _NativeRunBackend  # noqa: E402


_DEFAULT_STDERR_LIMIT_V1 = 64 * 1024


@cache
def _mpfi_request(
    *,
    mpfr_value_mode: int = 0o644,
    max_stderr_bytes: int = _DEFAULT_STDERR_LIMIT_V1,
) -> receipt.MpfiPipelineRequestV1:
    source_lock, admitted, _entries = _admitted_closure(
        mpfr_value_mode=mpfr_value_mode,
    )
    sources = _workspace_sources()
    generated = _generated_formula()
    return receipt.MpfiPipelineRequestV1(
        source_lock,
        admitted,
        sources,
        generated,
        _limits_for_bundle(source_lock, admitted, sources, generated),
        _job(),
        mpfi_runtime.MpfiRuntimeBindingV1(
            mpfi_runtime.mpfi_runtime_profile_v1(),
            executor.ExecutionLimitsV1(
                16 * 1024 * 1024,
                16 * 1024 * 1024,
                4096,
                16 * 1024 * 1024,
                max_stderr_bytes,
                60_000_000_000,
                1024 * 1024 * 1024,
                1,
            ),
        ),
    )


def _built_mpfi(
    request: receipt.MpfiPipelineRequestV1,
    backend: _BuildBackend,
) -> receipt.MpfiSourceBoundEvaluatorReceiptV1:
    """Drive the real controller to a sealed receipt over a fixture backend."""

    run_backend = _NativeRunBackend()
    controller = receipt.MpfiSourceBoundControllerV1(
        Path("/usr/bin/docker"),
        Path("/sys/fs/cgroup/labcolors/proof"),
    )
    with (
        mock.patch.object(
            build_transport.NativeDockerBuildBackendV1,
            "probe",
            autospec=True,
            side_effect=lambda _self: backend.probe(),
        ),
        mock.patch.object(
            build_transport.NativeDockerBuildBackendV1,
            "run_build",
            autospec=True,
            side_effect=lambda _self, build_request: backend.run_build(build_request),
        ),
        mock.patch.object(
            executor.NativeLinuxBackendV1,
            "probe",
            autospec=True,
            side_effect=lambda _self, guard: run_backend.probe(guard),
        ),
        mock.patch.object(
            executor.NativeLinuxBackendV1,
            "run",
            autospec=True,
            side_effect=lambda _self, invocation, capability: run_backend.run(
                invocation,
                capability,
            ),
        ),
        mock.patch.object(receipt.executor, "enter_observer_cgroup_v1"),
    ):
        result = controller.execute(request)
    if type(result) is not receipt.MpfiSourceBoundEvaluatorReceiptV1:
        raise AssertionError(f"controller did not seal a receipt: {result!r}")
    return result


def _mpfi_backend(
    payload: bytes = b"mpfi-static-identity",
    **changes: object,
) -> _BuildBackend:
    binary = _static_elf(payload)
    return _BuildBackend(
        (binary, binary),
        probe=changes.pop(
            "probe",
            _docker_capability(mpfi_build.MPFI_BUILD_TRANSPORT_POLICY_V1),
        ),
        **changes,
    )


class _SharedDerivationProbe(Exception):
    """Distinct failure signal, recognised by its marker.

    The pre-check path lets it propagate.  The build path does not: the MPFI
    receipt's broad ``except Exception`` blocks convert it into a
    ``REPLAY_BINDING_FAILED`` rejection, so the shared-derivation test tells
    the probe apart by its marker landing verbatim in the rejection detail,
    not by the exception escaping the controller.
    """


class MpfiStaticSourceIdentityTests(unittest.TestCase):
    def test_expected_source_identity_equals_the_post_build_manifest(self) -> None:
        request = _mpfi_request()

        expected = receipt.expected_comparator_source_identity_v1(request)

        sealed = _built_mpfi(request, _mpfi_backend())
        manifest = sealed.comparator.manifest
        self.assertEqual(expected, manifest.source_identity)
        # Anti-vacuity: the source identity is a strict projection of the
        # manifest, so a derivation that accidentally returned the full
        # identity, the receipt's own source identity or a constant would
        # satisfy a weaker assertion and still be wrong.
        self.assertNotEqual(expected, manifest.identity)
        self.assertNotEqual(expected, sealed.evidence.source_identity)
        self.assertNotEqual(expected, request.build_sources.identity)
        self.assertIs(type(expected), bytes)
        self.assertEqual(len(expected), 32)
        self.assertNotEqual(expected, bytes(32))

    def test_expected_source_identity_folds_exactly_the_named_coordinates(self) -> None:
        request = _mpfi_request()
        sealed = _built_mpfi(request, _mpfi_backend(b"named-coordinates"))
        manifest = sealed.comparator.manifest.manifest

        coordinates = tuple(
            getattr(manifest, name)
            for name in protocol.source_bound_coordinates_v2()
        )

        self.assertEqual(len(coordinates), 8)
        self.assertEqual(
            receipt.expected_comparator_source_identity_v1(request),
            protocol.source_bound_identity_v2(
                protocol.ComparatorKindV1.MPFI,
                coordinates,
            ),
        )

    def test_expected_source_identity_is_blind_to_every_build_observation(self) -> None:
        # The load-bearing step: the pre-check runs before Docker exists, so
        # the derived value has to match a manifest produced by any conforming
        # backend, not only the one this test happens to drive.
        request = _mpfi_request()
        expected = receipt.expected_comparator_source_identity_v1(request)
        backends = (
            _mpfi_backend(b"first-binary"),
            _mpfi_backend(
                b"second-binary",
                reported_stderr=b"a different build console\n",
            ),
            _mpfi_backend(
                b"third-binary",
                probe=_docker_capability(
                    mpfi_build.MPFI_BUILD_TRANSPORT_POLICY_V1,
                    daemon_marker=b"a-different-daemon",
                ),
            ),
        )

        identities = []
        source_identities = []
        for index, backend in enumerate(backends):
            with self.subTest(backend=index):
                manifest = _built_mpfi(request, backend).comparator.manifest
                identities.append(manifest.identity)
                source_identities.append(manifest.source_identity)
                self.assertEqual(expected, manifest.source_identity)

        self.assertEqual(len(set(identities)), len(backends))
        self.assertEqual(set(source_identities), {expected})

    def test_expected_source_identity_moves_with_the_admitted_upstream_closure(self) -> None:
        drifted = _mpfi_request(mpfr_value_mode=0o755)

        baseline = receipt.expected_comparator_source_identity_v1(_mpfi_request())
        moved = receipt.expected_comparator_source_identity_v1(drifted)

        self.assertNotEqual(baseline, moved)
        self.assertEqual(
            moved,
            _built_mpfi(
                drifted,
                _mpfi_backend(b"drifted-closure"),
            ).comparator.manifest.source_identity,
        )

    def test_expected_source_identity_refuses_a_foreign_or_unbound_request(self) -> None:
        # A pre-check that answered for an unbound closure would certify an
        # identity no build could ever reproduce, which is worse than having
        # no pre-check at all.
        with self.assertRaises(receipt.MpfiRequestErrorV1) as wrong_type:
            receipt.expected_comparator_source_identity_v1(object())
        self.assertEqual(
            wrong_type.exception.reason,
            receipt.MpfiRequestErrorReasonV1.WRONG_TYPE,
        )

        request = _mpfi_request()
        foreign_lock, _foreign_sources, _entries = _admitted_closure(
            mpfr_value_mode=0o700,
        )
        mutated = tuple.__new__(
            receipt.MpfiPipelineRequestV1,
            (foreign_lock,) + tuple(request)[1:],
        )
        with self.assertRaises(receipt.MpfiRequestErrorV1) as foreign:
            receipt.expected_comparator_source_identity_v1(mutated)
        self.assertEqual(
            foreign.exception.reason,
            receipt.MpfiRequestErrorReasonV1.FOREIGN_SOURCE_CAPABILITY,
        )

    def test_the_build_and_the_pre_check_share_one_source_bound_derivation(self) -> None:
        # Two independent derivations of the same eight coordinates would
        # drift apart silently, and the pre-check would then approve an engine
        # the build does not produce.  Disabling the shared derivation has to
        # take both paths down; no coincidental copy survives that.
        request = _mpfi_request()
        marker = "single source-bound derivation"

        def _refuse(*_args: object, **_kwargs: object) -> object:
            raise _SharedDerivationProbe(marker)

        with mock.patch.object(receipt, "_source_bound_preimages_v1", _refuse):
            with self.assertRaisesRegex(_SharedDerivationProbe, marker):
                receipt.expected_comparator_source_identity_v1(request)
            with (
                mock.patch.object(
                    build_transport.NativeDockerBuildBackendV1,
                    "probe",
                    autospec=True,
                    side_effect=lambda _self: _mpfi_backend().probe(),
                ),
                mock.patch.object(
                    build_transport.NativeDockerBuildBackendV1,
                    "run_build",
                    autospec=True,
                    side_effect=lambda _self, build_request: _mpfi_backend().run_build(
                        build_request,
                    ),
                ),
            ):
                built = receipt.MpfiSourceBoundControllerV1(
                    Path("/usr/bin/docker"),
                    Path("/sys/fs/cgroup/labcolors/proof"),
                ).execute(request)
        self.assertIs(type(built), receipt.MpfiSourceBoundRejectedV1)
        self.assertEqual(
            built.reason,
            receipt.MpfiSourceBoundFailureReasonV1.REPLAY_BINDING_FAILED,
        )
        self.assertEqual(built.detail, marker)

        fresh = protocol.ComparatorManifestV2(
            protocol.ComparatorKindV1.MPFI,
            *(_digest(f"probe-coordinate-{index}") for index in range(10)),
        )
        with mock.patch.object(protocol, "source_bound_identity_v2", _refuse):
            with self.assertRaisesRegex(_SharedDerivationProbe, marker):
                receipt.expected_comparator_source_identity_v1(request)
            with self.assertRaisesRegex(_SharedDerivationProbe, marker):
                fresh.source_identity

    def test_the_source_bound_preimage_set_cannot_drift_from_the_manifest(self) -> None:
        # A coordinate added to the manifest joins the fold automatically; the
        # pre-check must then fail loudly instead of folding a stale eight.
        self.assertEqual(
            tuple(
                item.name
                for item in dataclass_fields(receipt.MpfiSourceBoundPreimagesV1)
            ),
            protocol.source_bound_coordinates_v2(),
        )
        for name in protocol.source_bound_coordinates_v2():
            with self.subTest(coordinate=name):
                self.assertNotIn(name, protocol.BUILD_OBSERVATION_COORDINATES_V2)


class MpfiSourceBoundPreimageGuardTests(unittest.TestCase):
    """Every refusal branch of the construction guard, proven reachable.

    Its neighbour above compares the field order against the protocol directly,
    which proves the invariant and not the guard: deleting the guard's body
    leaves that assertion green.  These prove the guard itself — including the
    schema-drift branch, whose loss no other suite can see because both sides
    read one declaration.
    """

    def _values(self) -> dict[str, bytes]:
        return self._values_for(protocol.source_bound_coordinates_v2())

    @staticmethod
    def _values_for(names: tuple[str, ...]) -> dict[str, bytes]:
        return {name: bytes([index + 1]) * 32 for index, name in enumerate(names)}

    def test_the_exact_shape_constructs(self) -> None:
        # Anti-vacuity: a guard that refused everything would satisfy both
        # refusal tests below and be worthless.
        built = receipt.MpfiSourceBoundPreimagesV1(**self._values())
        self.assertIs(type(built), receipt.MpfiSourceBoundPreimagesV1)

    def test_an_empty_or_foreign_preimage_is_refused(self) -> None:
        for name, bad in (("engine_release", b""), ("exclusions", "x" * 32)):
            with self.subTest(name=name):
                values = self._values()
                values[name] = bad
                with self.assertRaises(TypeError):
                    receipt.MpfiSourceBoundPreimagesV1(**values)

    def test_two_coordinates_sharing_bytes_are_refused(self) -> None:
        # Independent domain separation is the point: two coordinates folding
        # to the same bytes would make the identity blind to whichever of them
        # changed.
        values = self._values()
        values["evaluator_source"] = values["wrapper_source"]
        with self.assertRaises(TypeError):
            receipt.MpfiSourceBoundPreimagesV1(**values)

    def test_a_drifted_protocol_schema_is_refused_at_construction(self) -> None:
        # The third branch of the guard, and the one its docstring is about:
        # the field order is checked against the protocol rather than trusted.
        # Deleting that check leaves every suite green, because both sides read
        # the same declaration — so the drift is injected at the protocol seam.
        names = protocol.source_bound_coordinates_v2()
        for drifted in (
            tuple(reversed(names)),
            names + ("engine_release_v2",),
        ):
            with self.subTest(count=len(drifted)):
                with mock.patch.object(
                    receipt.protocol,
                    "source_bound_coordinates_v2",
                    lambda drifted=drifted: drifted,
                ):
                    with self.assertRaises(TypeError):
                        receipt.MpfiSourceBoundPreimagesV1(**self._values_for(names))


class SharedComparatorCoordinateSchemaTests(unittest.TestCase):
    """`source_bound_coordinates_v2()` has to serve both engines, or the dual
    pre-check cannot be written once.

    The two engines pin different upstreams — FLINT/Arb against GMP/MPFR/MPFI —
    and derive the coordinate *contents* from different sources.  The manifest
    grammar is nevertheless single: `kind` is a discriminator field, not a
    schema switch, so the coordinate names and their order are shared by
    construction rather than by agreement between two lists.
    """

    def test_one_coordinate_schema_serves_arb_and_mpfi(self) -> None:
        manifest_names = tuple(
            item.name
            for item in dataclass_fields(protocol.ComparatorManifestV2)
            if item.name != "kind"
        )

        self.assertEqual(
            tuple(
                item.name
                for item in dataclass_fields(receipt.MpfiComparatorPreimagesV1)
            ),
            manifest_names,
        )
        self.assertEqual(
            tuple(
                item.name
                for item in dataclass_fields(arb_pipeline.ArbComparatorPreimagesV1)
            ),
            manifest_names,
        )
        self.assertEqual(
            protocol.source_bound_coordinates_v2(),
            tuple(
                name
                for name in manifest_names
                if name not in protocol.BUILD_OBSERVATION_COORDINATES_V2
            ),
        )

    def test_the_fold_separates_the_two_engines_on_identical_coordinates(self) -> None:
        # A dual proof asserts two *distinct* comparator source identities.
        # If the fold did not bind the kind, two engines built from coincident
        # coordinates would collapse into one and the distinctness check would
        # pass on a single engine.
        coordinates = tuple(
            bytes([index + 1]) * 32
            for index in range(len(protocol.source_bound_coordinates_v2()))
        )
        self.assertNotEqual(
            protocol.source_bound_identity_v2(
                protocol.ComparatorKindV1.MPFI,
                coordinates,
            ),
            protocol.source_bound_identity_v2(
                protocol.ComparatorKindV1.ARB,
                coordinates,
            ),
        )


class ComparatorRuntimeBindingAsymmetryTests(unittest.TestCase):
    """Named measurement, not an endorsement: the two engines disagree about
    whether the runtime binding belongs to the comparator.

    MPFI folds `runtime_binding_identity_v1` into `arithmetic_input_set`, so
    replacing an executor limit moves the MPFI comparator source identity.
    Arb's comparator does not read the runtime binding at all, so the same
    replacement leaves the Arb source identity fixed.

    This matters because the decision chain carries `source_identity` exactly
    so a lane computed under one run is admissible under another run of the
    same sources (PROTOCOL.md, `source_identity`).  Under this asymmetry that
    portability holds for Arb across a runtime-limit change and fails for
    MPFI.  Which side is right is a protocol-semantics decision and is not
    settled here; this test exists so the difference cannot be changed on
    either side without the change being deliberate.
    """

    def test_only_mpfi_binds_the_runtime_binding_into_its_comparator(self) -> None:
        baseline = receipt.expected_comparator_source_identity_v1(_mpfi_request())
        relimited = receipt.expected_comparator_source_identity_v1(
            _mpfi_request(max_stderr_bytes=32 * 1024),
        )
        self.assertNotEqual(baseline, relimited)

        arb_baseline = self._arb_source_identity(_arb_request())
        arb_relimited = self._arb_source_identity(
            _arb_request(
                runtime_binding=_arb_runtime_binding(max_stderr_bytes=32 * 1024),
            ),
        )
        self.assertEqual(arb_baseline, arb_relimited)

    @staticmethod
    def _arb_source_identity(request: object) -> bytes:
        binary = _static_elf(b"arb-runtime-binding-probe")
        built = arb_pipeline.ControlledPipelineV1(
            build_backend=_BuildBackend((binary, binary)),
        ).build(request)
        if type(built) is not arb_pipeline.DiagnosticBuildObservationV1:
            raise AssertionError(f"Arb build did not observe: {built!r}")
        return built.comparator.manifest.source_identity


if __name__ == "__main__":
    unittest.main(verbosity=2)
