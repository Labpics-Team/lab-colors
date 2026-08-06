#!/usr/bin/env python3
"""Lane dispatch coordinator for the V5b2d full-domain RUN (V5b2d-1e).

The full 2^24 replay executes as independent GitHub Actions lanes, one per
packing-aligned window of the exact full manifest.  The coordinator derives
the canonical lane plan — contiguous windows of a fixed lane width that
cover [0, 2^24) exactly, with every seam landing on the packing alignment
and on a shard boundary — and turns it into one `gh workflow run` invocation
per lane of `full-domain-corpus.yml`.  Widths that cannot produce an exact
aligned cover are typed rejections before any dispatch exists; `--dry-run`
prints every dispatch command without touching the network.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

PROOF = Path(__file__).resolve().parent
sys.path.insert(0, str(PROOF))

import corpus  # noqa: E402
import corpus_lane  # noqa: E402
import region_proof_protocol as protocol  # noqa: E402

PLAN_SCHEMA_V1 = "corpus-dispatch-plan-v1"
WORKFLOW_V1 = "full-domain-corpus.yml"
VERIFICATION_WORKFLOW_V1 = "verification-lanes.yml"
# One evidence run carries one artifact per engine; a lane that picked an
# artifact by directory order would silently replay the foreign engine's
# bundle, so dispatch binds the exact artifact name from this allowlist.
EVIDENCE_ARTIFACTS_V1 = (
    "verification-evidence-arb",
    "verification-evidence-mpfi",
)
DEFAULT_LANE_WIDTH = 1 << 16
DEFAULT_SHARD_WIDTH = corpus_lane.DEFAULT_SHARD_POINTS
FULL_DOMAIN = protocol.OUTPUT_CARDINALITY_V1
ALIGNMENT = corpus.CORPUS_SHARD_ALIGNMENT_V1


def lane_plan_v1(
    lane_width: int = DEFAULT_LANE_WIDTH,
    shard_width: int = DEFAULT_SHARD_WIDTH,
) -> tuple[tuple[int, int], ...] | corpus.ShardCorpusRejectedV1:
    """Contiguous lane windows covering the full sRGB8 ordinal space exactly.

    Every window is `lane_width` points wide, starts on a packing-aligned
    ordinal, and is an exact multiple of the shard width, so each lane can
    run the sharded corpus runner standalone and the lanes concatenate into
    the exact monolithic transcript.
    """

    for name, value in (("lane_width", lane_width), ("shard_width", shard_width)):
        if (
            type(value) is not int
            or value < ALIGNMENT
            or value % ALIGNMENT != 0
        ):
            return corpus._reject(
                corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
                f"{name} must be a positive multiple of the packing alignment",
            )
    if lane_width > FULL_DOMAIN:
        return corpus._reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            "lane width exceeds the full sRGB8 ordinal space",
        )
    if FULL_DOMAIN % lane_width != 0:
        return corpus._reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            "lane width does not divide the full domain exactly",
        )
    if lane_width % shard_width != 0:
        return corpus._reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            "shard width must divide the lane width exactly",
        )
    return tuple(
        (start, lane_width) for start in range(0, FULL_DOMAIN, lane_width)
    )


def plan_json_v1(
    lane_width: int = DEFAULT_LANE_WIDTH,
    shard_width: int = DEFAULT_SHARD_WIDTH,
) -> str | corpus.ShardCorpusRejectedV1:
    """The deterministic wire form of one lane plan."""

    plan = lane_plan_v1(lane_width, shard_width)
    if type(plan) is not tuple:
        return plan
    return json.dumps(
        {
            "schema": PLAN_SCHEMA_V1,
            "lane_width": lane_width,
            "shard_width": shard_width,
            "lane_count": len(plan),
            "domain_points": FULL_DOMAIN,
            "lanes": [
                {"window_start": start, "window_points": points}
                for start, points in plan
            ],
        },
        sort_keys=True,
        separators=(",", ":"),
    )


def dispatch_commands_v1(
    plan: tuple[tuple[int, int], ...] | corpus.ShardCorpusRejectedV1,
    shard_width: int,
) -> tuple[tuple[str, ...], ...] | corpus.ShardCorpusRejectedV1:
    """One `gh workflow run` invocation per lane window of the plan."""

    if type(plan) is not tuple:
        return plan
    return tuple(
        (
            "gh",
            "workflow",
            "run",
            WORKFLOW_V1,
            "-f",
            f"window_start={start}",
            "-f",
            f"window_points={points}",
            "-f",
            f"shard_points={shard_width}",
        )
        for start, points in plan
    )


def verification_dispatch_commands_v1(
    plan: tuple[tuple[int, int], ...] | corpus.ShardCorpusRejectedV1,
    shard_width: int,
    evidence_run_id: object,
    evidence_artifact: object,
) -> tuple[tuple[str, ...], ...] | corpus.ShardCorpusRejectedV1:
    """One verification lane dispatch per plan window, bound to the evidence run.

    The verification lanes replay the engine transcript's domain under the
    engine's own comparator, so every lane names the run that carries the
    engine verification evidence (job bytes and comparator bundle) as a
    dispatch coordinate; the evidence run id must be a positive integer.
    One run carries one artifact per engine, so every lane must also name
    the exact engine artifact it replays; foreign artifact names are typed
    rejections before any dispatch exists.
    """

    if type(plan) is not tuple:
        return plan
    if type(evidence_run_id) is not int or evidence_run_id <= 0:
        return corpus._reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            "verification dispatch must bind a positive evidence run id",
        )
    if (
        type(evidence_artifact) is not str
        or evidence_artifact not in EVIDENCE_ARTIFACTS_V1
    ):
        return corpus._reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            "verification dispatch must bind an allowlisted engine artifact",
        )
    return tuple(
        (
            "gh",
            "workflow",
            "run",
            VERIFICATION_WORKFLOW_V1,
            "-f",
            f"evidence_run_id={evidence_run_id}",
            "-f",
            f"evidence_artifact={evidence_artifact}",
            "-f",
            f"window_start={start}",
            "-f",
            f"window_points={points}",
            "-f",
            f"shard_points={shard_width}",
        )
        for start, points in plan
    )


def gh_run_artifact_names_v1(run_id: int) -> tuple[str, ...]:
    """Artifact names the given workflow run carries, read through `gh`.

    The one impure boundary of this admission: it observes GitHub.  It is
    injected into `admit_evidence_artifact_v1` so the decision itself stays
    testable without a network.
    """

    completed = subprocess.run(
        (
            "gh",
            "api",
            f"repos/{{owner}}/{{repo}}/actions/runs/{run_id}/artifacts",
            "--jq",
            ".artifacts[].name",
        ),
        capture_output=True,
        text=True,
        check=True,
    )
    return tuple(
        line.strip() for line in completed.stdout.splitlines() if line.strip()
    )


def admit_evidence_artifact_v1(
    evidence_run_id: int,
    evidence_artifact: str,
    observer: object = gh_run_artifact_names_v1,
) -> corpus.ShardCorpusRejectedV1 | None:
    """Refuse a dispatch whose evidence run cannot carry what it names.

    Every lane downloads `evidence_artifact` from `evidence_run_id`; a run
    that does not exist, or carries no such artifact, turns one mistake into
    256 doomed jobs.  A positive integer is not evidence that a run exists,
    so the coordinator asks before it dispatches, and any failure to observe
    is itself a refusal — never a crash and never a silent proceed.
    """

    try:
        names = tuple(observer(evidence_run_id))  # type: ignore[operator]
    except Exception:
        # The observation is a hostile boundary: an unreachable run, a
        # missing token or a malformed reply must all land as one refusal.
        return corpus._reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            f"verification dispatch cannot observe run {evidence_run_id}",
        )
    if evidence_artifact not in names:
        return corpus._reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            f"run {evidence_run_id} carries no artifact {evidence_artifact!r}",
        )
    return None


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--mode",
        choices=("plan", "dispatch", "verification-dispatch"),
        required=True,
    )
    parser.add_argument("--lane-width", type=int, default=DEFAULT_LANE_WIDTH)
    parser.add_argument("--shard-width", type=int, default=DEFAULT_SHARD_WIDTH)
    parser.add_argument("--evidence-run-id", type=int, default=None)
    parser.add_argument("--evidence-artifact", type=str, default=None)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)

    plan = lane_plan_v1(args.lane_width, args.shard_width)
    if type(plan) is not tuple:
        print(
            f"lane plan rejected: lane_width={args.lane_width} "
            f"shard_width={args.shard_width} ({plan!r})",
            file=sys.stderr,
        )
        return 64

    if args.mode == "plan":
        args.out.mkdir(parents=True, exist_ok=True)
        (args.out / "dispatch-plan.json").write_bytes(
            plan_json_v1(args.lane_width, args.shard_width).encode("ascii")
        )
        print(
            f"plan lanes={len(plan)} lane_width={args.lane_width} "
            f"shard_width={args.shard_width} domain_points={FULL_DOMAIN}"
        )
        return 0

    if args.mode == "verification-dispatch":
        if args.evidence_run_id is None:
            print(
                "verification dispatch requires --evidence-run-id",
                file=sys.stderr,
            )
            return 64
        if args.evidence_artifact is None:
            print(
                "verification dispatch requires --evidence-artifact",
                file=sys.stderr,
            )
            return 64
        commands = verification_dispatch_commands_v1(
            plan,
            args.shard_width,
            args.evidence_run_id,
            args.evidence_artifact,
        )
        if type(commands) is tuple and not args.dry_run:
            # Fail closed before the first dispatch: a dry run stays offline
            # by contract, so the observation belongs to the live path only.
            refusal = admit_evidence_artifact_v1(
                args.evidence_run_id, args.evidence_artifact
            )
            if refusal is not None:
                print(f"verification dispatch refused: {refusal.detail}", file=sys.stderr)
                return 64
    else:
        commands = dispatch_commands_v1(plan, args.shard_width)
    for command in commands:
        rendered = " ".join(command)
        if args.dry_run:
            print(rendered)
            continue
        subprocess.run(command, check=True)
        print(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
