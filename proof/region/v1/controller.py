#!/usr/bin/env python3
"""Bounded fixture admission for the offline region-proof protocol."""

from __future__ import annotations

import argparse
import hashlib
import itertools
import json
import os
import stat
import sys
from dataclasses import dataclass
from pathlib import Path

from region_proof_protocol import (
    FORMULA_RELEASE_DOMAIN_V1,
    ComparatorKindV1,
    ContextualRegionDefinitionV1,
    ProofJobV1,
    ProofPolicyV1,
    ProtocolErrorV1,
    ReducedDomainManifestV1,
)


@dataclass(frozen=True)
class FrozenInputV1:
    relative_path: str
    length: int
    sha256: str


# These coordinates freeze the first real V5b2b protocol corpus. They are
# replay gates, not performance observations or colour-science constants.
FROZEN_INPUTS_V1 = (
    FrozenInputV1(
        "crates/labcolors-core/contracts/contextual-region-formula-v1.lcir",
        24_434,
        "a6f77ac462f226453b1c27bbd8637b62780b9a640c317a6f50028dacd1de8540",
    ),
    FrozenInputV1(
        "proof/region/v1/fixtures/v5b2b-definition-0a8d1c3d.bin",
        451,
        "0a8d1c3d2f0052be84b5783071699861aad0ac83dae62de3275267754681cdc9",
    ),
    FrozenInputV1(
        "proof/region/v1/fixtures/reduced-domain-srgb8-seams-v1.bin",
        1_785,
        "1e65d51b2be490f4c76bfbcb99656ffba303481b12f8163b1013575f9a58d0d9",
    ),
    FrozenInputV1(
        "proof/region/v1/fixtures/proof-policy-protocol-v1.bin",
        68,
        "6bee2ffbcfda079612b3837ea5d79a1c7ece2590ef155a61211ee44aae290947",
    ),
    FrozenInputV1(
        "proof/region/v1/fixtures/proof-job-v1.bin",
        26_906,
        "149d55c811ac5ed2e942fbfde259b4049b9fd44c56ad77329fcb14a45efee573",
    ),
)


class ControllerErrorV1(RuntimeError):
    pass


def _read_frozen(repo_root: Path, item: FrozenInputV1) -> bytes:
    try:
        root = repo_root.resolve(strict=True)
    except (OSError, RuntimeError):
        raise ControllerErrorV1("repository root is unavailable") from None
    if not root.is_dir():
        raise ControllerErrorV1("repository root is not a directory")

    relative = Path(item.relative_path)
    if relative.is_absolute() or not relative.parts or any(
        part in ("", ".", "..") for part in relative.parts
    ):
        raise ControllerErrorV1(f"invalid frozen input path: {item.relative_path}")
    path = root / relative
    try:
        resolved = path.resolve(strict=True)
    except (OSError, RuntimeError):
        raise ControllerErrorV1(
            f"frozen input is unavailable: {item.relative_path}"
        ) from None
    if resolved != path:
        raise ControllerErrorV1(f"symlinked input path: {item.relative_path}")
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(resolved, flags)
        try:
            metadata = os.fstat(descriptor)
            if not stat.S_ISREG(metadata.st_mode):
                raise ControllerErrorV1(
                    f"not a regular frozen input: {item.relative_path}"
                )
            if metadata.st_size != item.length:
                raise ControllerErrorV1(
                    f"length mismatch for {item.relative_path}: {metadata.st_size} != {item.length}"
                )
            with os.fdopen(descriptor, "rb", closefd=False) as source:
                data = source.read(item.length + 1)
        finally:
            os.close(descriptor)
    except OSError:
        raise ControllerErrorV1(
            f"frozen input cannot be read: {item.relative_path}"
        ) from None
    if len(data) != item.length:
        raise ControllerErrorV1(f"bounded read drifted for {item.relative_path}")
    actual = hashlib.sha256(data).hexdigest()
    if actual != item.sha256:
        raise ControllerErrorV1(
            f"digest mismatch for {item.relative_path}: {actual} != {item.sha256}"
        )
    return data


def verify_fixtures(repo_root: Path) -> dict[str, int | str]:
    try:
        root = repo_root.resolve(strict=True)
    except (OSError, RuntimeError):
        raise ControllerErrorV1("repository root is unavailable") from None
    if not root.is_dir():
        raise ControllerErrorV1("repository root is not a directory")
    admitted = {
        item.relative_path: _read_frozen(root, item) for item in FROZEN_INPUTS_V1
    }
    formula = admitted[FROZEN_INPUTS_V1[0].relative_path]
    definition_raw = admitted[FROZEN_INPUTS_V1[1].relative_path]
    domain_raw = admitted[FROZEN_INPUTS_V1[2].relative_path]
    policy_raw = admitted[FROZEN_INPUTS_V1[3].relative_path]
    job_raw = admitted[FROZEN_INPUTS_V1[4].relative_path]

    try:
        definition = ContextualRegionDefinitionV1.parse(definition_raw)
        domain = ReducedDomainManifestV1.parse(domain_raw)
        policy = ProofPolicyV1.parse(policy_raw)
        job = ProofJobV1.parse(job_raw)
    except ProtocolErrorV1 as error:
        raise ControllerErrorV1(f"frozen protocol input was rejected: {error}") from None

    formula_release = hashlib.sha256(
        FORMULA_RELEASE_DOMAIN_V1 + len(formula).to_bytes(8, "big") + formula
    ).digest()
    if definition.formula_release != formula_release:
        raise ControllerErrorV1("definition does not bind the frozen formula")

    values = (0, 1, 10, 11, 127, 128, 254, 255)
    expected_ordinals = (
        (red << 16) | (green << 8) | blue
        for red, green, blue in itertools.product(values, repeat=3)
    )
    sentinel = object()
    if domain.point_count != len(values) ** 3 or any(
        actual != expected
        for actual, expected in itertools.zip_longest(
            domain.iter_ordinals(), expected_ordinals, fillvalue=sentinel
        )
    ):
        raise ControllerErrorV1("reduced domain is not the frozen seam cube")

    if tuple(item.kind for item in policy.comparators) != (
        ComparatorKindV1.ARB,
        ComparatorKindV1.MPFI,
    ):
        raise ControllerErrorV1("frozen comparator order drifted")
    if any(
        item.precision_ladder != (64, 128)
        or item.per_point_work != 0
        or item.global_pregrant != 0
        for item in policy.comparators
    ):
        raise ControllerErrorV1("frozen zero-grant hostile policy drifted")

    if (
        job.definition.encode() != definition_raw
        or job.formula_spec != formula
        or job.domain.encode() != domain_raw
        or job.policy.encode() != policy_raw
        or job.encode() != job_raw
    ):
        raise ControllerErrorV1("proof job nested binding drifted")

    return {
        "admitted_files": len(admitted),
        "definition_fields": len(definition.fields),
        "domain_points": domain.point_count,
        "formula_bytes": len(formula),
        "job_bytes": len(job_raw),
        "status": "protocol-fixtures-admitted",
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    subcommands = parser.add_subparsers(dest="command", required=True)
    verify = subcommands.add_parser("verify-fixtures")
    verify.add_argument("--repo-root", type=Path, required=True)
    arguments = parser.parse_args(argv)

    if arguments.command == "verify-fixtures":
        try:
            evidence = verify_fixtures(arguments.repo_root)
        except ControllerErrorV1 as error:
            print(
                json.dumps(
                    {
                        "error": str(error),
                        "status": "protocol-fixtures-rejected",
                    },
                    sort_keys=True,
                ),
                file=sys.stderr,
            )
            return 1
        print(json.dumps(evidence, sort_keys=True))
        return 0
    raise AssertionError("argparse admitted an unknown command")


if __name__ == "__main__":
    raise SystemExit(main())
