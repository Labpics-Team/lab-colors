#!/usr/bin/env python3
"""Hostile tests for source-origin observations."""

from __future__ import annotations

import hashlib
import gzip
import io
import signal
import sys
import tarfile
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace
from unittest import mock


PROOF = Path(__file__).resolve().parents[2]
ARB = PROOF / "arb"
sys.path.insert(0, str(PROOF))
sys.path.insert(0, str(ARB))

import origin  # noqa: E402
import provenance  # noqa: E402


def git_content_relation_fixture() -> tuple[
    provenance.SourceReleaseLockV1,
    provenance.SafeSourceArchiveV1,
    origin.GitTreeProcessObservationV1,
]:
    common_body = b"license"
    omitted_body = b"ci"
    generated_body = b"config"
    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        root = tarfile.TarInfo("fixture-1/")
        root.type = tarfile.DIRTYPE
        root.mode = 0o755
        root.mtime = 0
        archive.addfile(root)
        for name, body, mode in (
            ("fixture-1/LICENSE", common_body, 0o644),
            ("fixture-1/configure", generated_body, 0o755),
        ):
            member = tarfile.TarInfo(name)
            member.mode = mode
            member.size = len(body)
            member.mtime = 0
            archive.addfile(member, io.BytesIO(body))
    archive_bytes = gzip.compress(raw.getvalue(), compresslevel=9, mtime=0)
    lock = provenance.SourceReleaseLockV1(
        provenance.SourceRoleV1.FLINT_ARB,
        "1",
        "https://example.invalid/fixture-1.tar.gz",
        provenance.ArchiveFormatV1.TAR_GZIP,
        len(archive_bytes),
        hashlib.sha256(archive_bytes).digest(),
        len(raw.getvalue()),
        "fixture-1/",
        2,
        len(common_body) + len(generated_body),
        (
            provenance.LegalFileV1(
                "LICENSE", len(common_body), hashlib.sha256(common_body).digest()
            ),
        ),
        provenance.GitContentRelationPolicyV1(
            "https://example.invalid/fixture.git",
            "v1",
            bytes.fromhex("11" * 20),
            bytes.fromhex("22" * 20),
            1,
            (".github/ci.yml",),
            (
                provenance.ProjectPinnedReleaseOnlyFileV1(
                    "configure",
                    0o755,
                    len(generated_body),
                    hashlib.sha256(generated_body).digest(),
                ),
            ),
        ),
    )
    process = origin.GitTreeProcessObservationV1(
        lock.integrity.commit,
        lock.integrity.tree,
        bytes.fromhex("55" * 32),
        (
            origin.FileCoordinateV1(
                ".github/ci.yml", 0o644, len(omitted_body), hashlib.sha256(omitted_body).digest()
            ),
            origin.FileCoordinateV1(
                "LICENSE", 0o644, len(common_body), hashlib.sha256(common_body).digest()
            ),
        ),
        bytes.fromhex("33" * 32),
        bytes.fromhex("44" * 32),
        _token=origin._GIT_PROCESS_TOKEN,
    )
    return lock, provenance.admit_source_archive(lock, archive_bytes), process


def signed_source_fixture() -> tuple[
    provenance.SourceReleaseLockV1,
    provenance.SafeSourceArchiveV1,
]:
    base, admitted, _process = git_content_relation_fixture()
    packets = origin.decode_public_key_armour((ARB / "keys/gmp.asc").read_bytes())
    signed = replace(
        base,
        role=provenance.SourceRoleV1.GMP,
        integrity=provenance.DetachedSignaturePolicyV1(
            "https://example.invalid/fixture-1.tar.gz.sig",
            len(b"signature"),
            hashlib.sha256(b"signature").digest(),
            hashlib.sha256(packets).digest(),
            bytes.fromhex("343c2ff0fbee5ec2edbef399f3599ff828c67298"),
        ),
    )
    return signed, provenance.admit_source_archive(signed, admitted.archive_bytes)


class PublicKeyArmourTests(unittest.TestCase):
    def test_pinned_key_armour_decodes_to_exact_openpgp_packets(self) -> None:
        cases = (
            (
                "gmp.asc",
                "928ac84aa0e2134bbb335cd439110dc3f9b967eb04caff4a44dd5d04a3f13474",
            ),
            (
                "mpfr.asc",
                "3fe00f68bbf3888ae185b950d4db0f708dd01b6159cb03dec77296f9045b6372",
            ),
        )
        for name, expected in cases:
            with self.subTest(name=name):
                packets = origin.decode_public_key_armour((ARB / "keys" / name).read_bytes())
                self.assertEqual(hashlib.sha256(packets).hexdigest(), expected)

    def test_armour_is_canonical_and_crc_checked(self) -> None:
        valid = (ARB / "keys" / "mpfr.asc").read_bytes()
        mutants = (
            valid + b"\n",
            valid.replace(b"=3az7", b"=3az8", 1),
            valid.replace(b"PUBLIC KEY", b"PRIVATE KEY", 1),
            valid.replace(b"\n\n", b"\nComment: ambient\n\n", 1),
            valid.replace(b"\n", b"\r\n", 1),
        )
        for mutant in mutants:
            with self.subTest(mutant=hashlib.sha256(mutant).hexdigest()):
                with self.assertRaises(origin.OriginErrorV1):
                    origin.decode_public_key_armour(mutant)


class _DiagnosticRunner:
    def __init__(
        self,
        *observations: origin.DiagnosticProcessObservationV1 | object,
    ) -> None:
        self.observations = list(observations)
        self.requests: list[origin.DiagnosticProcessRequestV1] = []

    def run(
        self,
        request: origin.DiagnosticProcessRequestV1,
    ) -> origin.DiagnosticProcessObservationV1:
        self.requests.append(request)
        if not self.observations:
            raise RuntimeError("unexpected diagnostic invocation")
        return self.observations.pop(0)  # type: ignore[return-value]


class DiagnosticProcessBoundaryTests(unittest.TestCase):
    def test_core_has_no_builtin_process_or_process_group_runner(self) -> None:
        self.assertFalse(hasattr(origin, "_run_bounded"))
        self.assertFalse(hasattr(origin, "subprocess"))
        self.assertFalse(hasattr(origin, "selectors"))

    def test_client_owned_diagnostic_bytes_remain_bounded_and_untrusted(self) -> None:
        request = origin.DiagnosticProcessRequestV1(
            ("verifier", "--version"),
            None,
            Path("/"),
            {"LANG": "C"},
            (),
            1,
            4,
            4,
        )
        cases = (
            (
                _DiagnosticRunner(
                    origin.DiagnosticProcessObservationV1(0, b"12345", b"")
                ),
                origin.OriginReasonV1.VERIFIER_OUTPUT_LIMIT,
            ),
            (
                _DiagnosticRunner(SimpleNamespace(returncode=0, stdout=b"", stderr=b"")),
                origin.OriginReasonV1.VERIFIER_UNAVAILABLE,
            ),
        )
        for runner, reason in cases:
            with self.subTest(reason=reason):
                with self.assertRaises(origin.OriginErrorV1) as caught:
                    origin._observe_diagnostic_process_v1(runner, request)
                self.assertEqual(caught.exception.reason, reason)

    def test_diagnostic_runner_receives_every_resource_bound_explicitly(self) -> None:
        observed = origin.DiagnosticProcessObservationV1(0, b"ok", b"")
        runner = _DiagnosticRunner(observed)
        request = origin.DiagnosticProcessRequestV1(
            ("verifier", "arg"),
            b"input",
            Path("/tmp"),
            {"LANG": "C", "TZ": "UTC"},
            (7,),
            3,
            8,
            9,
        )

        actual = origin._observe_diagnostic_process_v1(runner, request)

        self.assertIs(actual, observed)
        self.assertEqual(runner.requests, [request])
class GpgStatusTests(unittest.TestCase):
    FINGERPRINT = bytes.fromhex("343c2ff0fbee5ec2edbef399f3599ff828c67298")

    def test_historical_signature_status_is_accepted_despite_later_key_expiry(self) -> None:
        status = b"""[GNUPG:] NEWSIG
[GNUPG:] KEYEXPIRED 1736961163
[GNUPG:] KEY_CONSIDERED 343C2FF0FBEE5EC2EDBEF399F3599FF828C67298 0
[GNUPG:] EXPKEYSIG F3599FF828C67298 Niels Moller
[GNUPG:] VALIDSIG 343C2FF0FBEE5EC2EDBEF399F3599FF828C67298 2023-07-30 1690719513 0 4 0 1 10 00 343C2FF0FBEE5EC2EDBEF399F3599FF828C67298
"""
        observed = origin.parse_gpgv_status(status, self.FINGERPRINT)

        self.assertIs(type(observed), origin.AcceptedHistoricalSignatureStatusV1)
        self.assertEqual(observed.signer_fingerprint, self.FINGERPRINT)
        self.assertEqual(observed.signature_unix_time, 1_690_719_513)

    def test_failure_wrong_signer_or_multiple_signatures_are_rejected(self) -> None:
        valid = b"""[GNUPG:] NEWSIG
[GNUPG:] VALIDSIG 343C2FF0FBEE5EC2EDBEF399F3599FF828C67298 2023-07-30 1690719513 0 4 0 1 10 00 343C2FF0FBEE5EC2EDBEF399F3599FF828C67298
"""
        cases = (
            valid.replace(self.FINGERPRINT.hex().upper().encode(), b"A" * 40),
            valid.replace(b"2023-07-30", b"2023-07-31", 1),
            valid + valid,
            valid.replace(b"VALIDSIG", b"BADSIG  ", 1),
            valid + b"[GNUPG:] FAILURE verify 17\n",
            valid + b"unframed stdout\n",
        )
        for status in cases:
            with self.subTest(status=hashlib.sha256(status).hexdigest()):
                with self.assertRaises(origin.OriginErrorV1):
                    origin.parse_gpgv_status(status, self.FINGERPRINT)

    def test_unbounded_timestamp_and_zero_fingerprint_fail_as_typed_input(self) -> None:
        valid = b"""[GNUPG:] NEWSIG
[GNUPG:] VALIDSIG 343C2FF0FBEE5EC2EDBEF399F3599FF828C67298 2023-07-30 1690719513 0 4 0 1 10 00 343C2FF0FBEE5EC2EDBEF399F3599FF828C67298
"""
        cases = (
            (valid.replace(b"1690719513", b"9" * 400), self.FINGERPRINT),
            (valid.replace(self.FINGERPRINT.hex().upper().encode(), b"0" * 40), bytes(20)),
        )
        for status, fingerprint in cases:
            with self.subTest(status=hashlib.sha256(status).hexdigest()):
                with self.assertRaises(origin.OriginErrorV1):
                    origin.parse_gpgv_status(status, fingerprint)

    def test_signature_observation_is_explicitly_historical_and_diagnostic(self) -> None:
        status = b"""[GNUPG:] NEWSIG
[GNUPG:] VALIDSIG 343C2FF0FBEE5EC2EDBEF399F3599FF828C67298 2023-07-30 1690719513 0 4 0 1 10 00 343C2FF0FBEE5EC2EDBEF399F3599FF828C67298
"""
        signature = b"signature"
        expected, admitted = signed_source_fixture()
        process = origin.GpgvProcessObservationV1(
            0,
            status,
            b"",
            admitted.tree_identity,
            admitted.archive_sha256,
            hashlib.sha256(signature).digest(),
            expected.integrity.public_key_packets_sha256,
            bytes.fromhex("11" * 32),
            bytes.fromhex("22" * 32),
            _token=origin._GPGV_PROCESS_TOKEN,
        )
        observed = origin.admit_detached_signature_observation(
            expected=expected,
            admitted=admitted,
            signature=signature,
            public_key_armour=(ARB / "keys/gmp.asc").read_bytes(),
            process=process,
        )

        self.assertIs(
            type(observed),
            origin.HistoricalPathRecheckedSignatureDiagnosticV1,
        )
        for attribute in (
            "authenticated_source",
            "current_publisher",
            "currently_trusted",
            "publisher",
            "verified_publisher",
        ):
            with self.subTest(attribute=attribute):
                self.assertFalse(hasattr(observed, attribute))

    def test_process_observation_cannot_report_other_source_bytes(self) -> None:
        status = b"""[GNUPG:] NEWSIG
[GNUPG:] VALIDSIG 343C2FF0FBEE5EC2EDBEF399F3599FF828C67298 2023-07-30 1690719513 0 4 0 1 10 00 343C2FF0FBEE5EC2EDBEF399F3599FF828C67298
"""
        expected, admitted = signed_source_fixture()
        process = origin.GpgvProcessObservationV1(
            0,
            status,
            b"",
            bytes.fromhex("ff" * 32),
            admitted.archive_sha256,
            hashlib.sha256(b"signature").digest(),
            expected.integrity.public_key_packets_sha256,
            bytes.fromhex("11" * 32),
            bytes.fromhex("22" * 32),
            _token=origin._GPGV_PROCESS_TOKEN,
        )
        with self.assertRaises(origin.OriginErrorV1) as caught:
            origin.admit_detached_signature_observation(
                expected=expected,
                admitted=admitted,
                signature=b"signature",
                public_key_armour=(ARB / "keys/gmp.asc").read_bytes(),
                process=process,
            )
        self.assertEqual(caught.exception.reason, origin.OriginReasonV1.COORDINATE_MISMATCH)

    def test_crashed_gpgv_is_a_typed_process_failure(self) -> None:
        _expected, admitted = signed_source_fixture()
        with tempfile.TemporaryDirectory() as temporary:
            executable = Path(temporary) / "gpgv"
            executable.write_bytes(b"diagnostic executable bytes")
            runner = _DiagnosticRunner(
                origin.DiagnosticProcessObservationV1(0, b"gpgv fixture\n", b""),
                origin.DiagnosticProcessObservationV1(-signal.SIGSEGV, b"", b""),
            )

            with self.assertRaises(origin.OriginErrorV1) as caught:
                origin.run_gpgv(
                    admitted,
                    b"signature",
                    (ARB / "keys/gmp.asc").read_bytes(),
                    executable=executable,
                    runner=runner,
                )
        self.assertEqual(caught.exception.reason, origin.OriginReasonV1.VERIFIER_FAILED)

    def test_process_and_signature_diagnostic_have_no_public_constructor(self) -> None:
        with self.assertRaises(TypeError):
            origin.GpgvProcessObservationV1(
                0,
                b"[GNUPG:] NEWSIG\n",
                b"",
                bytes.fromhex("88" * 32),
                bytes.fromhex("99" * 32),
                bytes.fromhex("aa" * 32),
                bytes.fromhex("bb" * 32),
                bytes.fromhex("11" * 32),
                bytes.fromhex("22" * 32),
                _token=object(),
            )
        with self.assertRaises(TypeError):
            origin.admit_detached_signature_observation(
                expected=signed_source_fixture()[0],
                admitted=signed_source_fixture()[1],
                signature=b"fake",
                public_key_armour=(ARB / "keys/gmp.asc").read_bytes(),
                process=SimpleNamespace(returncode=0, status=b"self report"),
            )
        with self.assertRaises(TypeError):
            origin.HistoricalPathRecheckedSignatureDiagnosticV1(
                bytes.fromhex("11" * 32),
                bytes.fromhex("22" * 32),
                bytes.fromhex("33" * 32),
                bytes.fromhex("77" * 32),
                self.FINGERPRINT,
                1,
                bytes.fromhex("44" * 32),
                bytes.fromhex("55" * 32),
                _token=object(),
            )

    def test_old_exact_or_current_authority_symbols_do_not_exist(self) -> None:
        for name in (
            "ExactGpgvSignatureObservationV1",
            "PathRecheckedSignatureObservationV1",
            "ValidSignatureObservationV1",
        ):
            with self.subTest(name=name):
                self.assertFalse(hasattr(origin, name))
        self.assertNotIn(
            "same_object_exec",
            origin.GpgvProcessObservationV1.__dataclass_fields__,
        )


class GitRelationTests(unittest.TestCase):
    def test_git_batch_recomputes_blob_object_identity(self) -> None:
        body = b"value"
        object_id = hashlib.sha1(b"blob 5\0" + body).hexdigest().encode("ascii")
        listing = ((object_id, "value", 0o644),)
        valid = object_id + b" blob 5\n" + body + b"\n"

        self.assertEqual(origin._parse_git_batch(valid, listing)[0].sha256, hashlib.sha256(body).digest())
        with self.assertRaises(origin.OriginErrorV1):
            origin._parse_git_batch(b"2" * 40 + valid[40:], ((b"2" * 40, "value", 0o644),))

    def test_recursive_tree_identity_has_an_independent_git_golden(self) -> None:
        listing = (
            (b"8c7e5a667f1b771847fe88c01c3de34413a1b220", "a.c", 0o644),
            (b"7371f47a6f8bd23a8fa1a8b2a9479cdd76380e54", "dir/b", 0o644),
        )
        self.assertEqual(
            origin._recompute_git_tree_identity(listing).hex(),
            "3930f0d390a7a4f2b29fde1dbc8abdc98a282fe0",
        )

    def test_deep_valid_git_tree_is_iterative_not_a_python_stack_overflow(self) -> None:
        body = b"z"
        object_id = hashlib.sha1(b"blob 1\0" + body).hexdigest().encode("ascii")
        path = "/".join(("a",) * 1_500 + ("z",))
        listing = origin._parse_git_listing(
            b"100644 blob " + object_id + b"\t" + path.encode("ascii") + b"\0"
        )

        tree = origin._recompute_git_tree_identity(listing)

        self.assertEqual(len(tree), 20)
        self.assertNotEqual(tree, bytes(20))

    def test_malformed_git_output_never_escapes_the_typed_boundary(self) -> None:
        for raw in (
            b"100644 blob " + b"0" * 40 + b"\tvalue\0",
            b"100644 blob " + b"1" * 40 + b"\t../escape\0",
            b"100644 blob " + b"1" * 40 + b"\ta\nb\0",
        ):
            with self.subTest(raw=raw):
                with self.assertRaises(origin.OriginErrorV1):
                    origin._parse_git_listing(raw)

    def test_git_batch_length_is_canonical_decimal(self) -> None:
        body = b"value"
        object_id = hashlib.sha1(b"blob 5\0" + body).hexdigest().encode("ascii")
        listing = ((object_id, "value", 0o644),)
        for length in (b"+5", b"05", b" 5"):
            with self.subTest(length=length):
                raw = object_id + b" blob " + length + b"\n" + body + b"\n"
                with self.assertRaises(origin.OriginErrorV1):
                    origin._parse_git_batch(raw, listing)

    def test_commit_identity_and_commit_to_tree_edge_are_recomputed(self) -> None:
        tree = bytes.fromhex("22" * 20)
        body = b"tree " + tree.hex().encode("ascii") + b"\nauthor A <a@example.test> 0 +0000\ncommitter A <a@example.test> 0 +0000\n\nrelease\n"
        commit = hashlib.sha1(b"commit " + str(len(body)).encode("ascii") + b"\0" + body).digest()

        self.assertEqual(origin._admit_git_commit_object(body, commit, tree), hashlib.sha256(body).digest())
        for changed_body, changed_commit, changed_tree in (
            (body + b"x", commit, tree),
            (body, bytes.fromhex("ff" * 20), tree),
            (body, commit, bytes.fromhex("ff" * 20)),
        ):
            with self.assertRaises(origin.OriginErrorV1):
                origin._admit_git_commit_object(changed_body, changed_commit, changed_tree)

    def test_archive_is_common_tree_plus_project_pinned_release_only_files(self) -> None:
        lock, admitted, process = git_content_relation_fixture()

        evidence = origin.admit_git_content_relation_observation(
            expected=lock,
            admitted=admitted,
            process=process,
        )

        self.assertEqual(evidence.common_file_count, 1)
        self.assertEqual(evidence.omitted_file_count, 1)
        self.assertEqual(evidence.project_pinned_release_only_file_count, 1)
        self.assertEqual(evidence.archive_sha256, lock.archive_sha256)
        self.assertIs(type(evidence), origin.RecomputedGitContentRelationV1)

    def test_any_relation_edge_mismatch_is_rejected(self) -> None:
        lock, admitted, process = git_content_relation_fixture()
        base = dict(expected=lock, admitted=admitted, process=process)
        mutations = (
            {"expected": replace(lock, version="2")},
            {
                "expected": replace(
                    lock,
                    integrity=replace(
                        lock.integrity,
                        commit=bytes.fromhex("ff" * 20),
                    ),
                )
            },
            {"process": SimpleNamespace(**process.__dict__)},
        )
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                with self.assertRaises((origin.OriginErrorV1, TypeError)):
                    origin.admit_git_content_relation_observation(**(base | mutation))

    def test_git_executable_metadata_has_no_relation_authority(self) -> None:
        lock, admitted, process = git_content_relation_fixture()
        other_verifier = origin.GitTreeProcessObservationV1(
            process.commit,
            process.tree,
            process.commit_object_sha256,
            process.files,
            bytes.fromhex("aa" * 32),
            bytes.fromhex("bb" * 32),
            _token=origin._GIT_PROCESS_TOKEN,
        )

        first = origin.admit_git_content_relation_observation(
            expected=lock, admitted=admitted, process=process
        )
        second = origin.admit_git_content_relation_observation(
            expected=lock, admitted=admitted, process=other_verifier
        )

        self.assertEqual(first, second)
        self.assertFalse(hasattr(first, "verifier_executable_sha256"))
        self.assertFalse(hasattr(first, "verifier_version_sha256"))

    def test_control_bytes_are_not_source_coordinates(self) -> None:
        for path in ("a\0b", "a\nb", "a\x7fb"):
            with self.subTest(path=repr(path)):
                with self.assertRaises(TypeError):
                    origin.FileCoordinateV1(
                        path,
                        0o644,
                        1,
                        hashlib.sha256(b"a").digest(),
                    )

    def test_process_and_verified_types_have_no_public_constructor(self) -> None:
        with self.assertRaises(TypeError):
            origin.GitTreeProcessObservationV1(
                bytes.fromhex("11" * 20),
                bytes.fromhex("22" * 20),
                bytes.fromhex("55" * 32),
                (origin.FileCoordinateV1("a", 0o644, 1, hashlib.sha256(b"a").digest()),),
                bytes.fromhex("33" * 32),
                bytes.fromhex("44" * 32),
                _token=object(),
            )
        with self.assertRaises(TypeError):
            origin.RecomputedGitContentRelationV1(
                bytes.fromhex("11" * 32),
                bytes.fromhex("22" * 32),
                bytes.fromhex("33" * 20),
                bytes.fromhex("44" * 20),
                bytes.fromhex("55" * 32),
                bytes.fromhex("66" * 32),
                bytes.fromhex("77" * 32),
                1,
                1,
                1,
                _token=object(),
            )

    def test_old_git_authority_symbols_do_not_exist(self) -> None:
        for name in (
            "ExactGitRelationObservationV1",
            "PathRecheckedGitRelationObservationV1",
            "admit_git_release_observation",
        ):
            with self.subTest(name=name):
                self.assertFalse(hasattr(origin, name))
        self.assertNotIn(
            "same_object_exec",
            origin.GitTreeProcessObservationV1.__dataclass_fields__,
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
