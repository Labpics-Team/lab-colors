#!/usr/bin/env python3
"""Lane dispatch coordinator for the V5b2d full-domain RUN (V5b2d-1e).

The full 2^24 replay executes as independent GitHub Actions lanes, one per
packing-aligned window of the exact full manifest.  The coordinator derives
the canonical lane plan — contiguous windows of a fixed lane width that
cover [0, 2^24) exactly, with every seam landing on the packing alignment
and on a shard boundary — and turns it into one `gh workflow run` invocation
per lane of `full-domain-corpus.yml`.  Widths that cannot produce an exact
aligned cover are typed rejections before any dispatch exists.  Printing the
commands is the default and dispatching is the opt-in `--execute`, which has
to name the campaign's size: the incident this coordinator exists to prevent
was a forgotten flag turning one mistake into 133 runs.
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


def gh_run_artifacts_v1(run_id: int) -> tuple[tuple[str, bool], ...]:
    """Every artifact the run lists, as `(name, expired)` pairs.

    The one impure boundary of this admission: it observes GitHub and reports
    what it saw, without deciding anything.  Which of those artifacts count is
    a rule, so it lives in `admit_evidence_artifact_v1` where a test can reach
    it — filtering here would put the rule behind the network.
    """

    completed = subprocess.run(
        (
            "gh",
            "api",
            "--paginate",
            f"repos/{{owner}}/{{repo}}/actions/runs/{run_id}/artifacts",
            "--jq",
            ".artifacts[] | [.name, (.expired // false)] | @tsv",
        ),
        capture_output=True,
        text=True,
        check=True,
    )
    return parse_artifact_listing_v1(completed.stdout)


def parse_artifact_listing_v1(stdout: str) -> tuple[tuple[str, bool], ...]:
    """Decode the artifact listing wire form into names and expiry.

    Reading `expired` is a rule like any other, so it lives here rather than
    behind the network where nothing can reach it.  A listing that does not
    look like the two-column form it is asked for raises: an unrecognised
    line silently read as "live" would admit exactly the stale evidence run
    the admission exists to refuse.
    """

    observed: list[tuple[str, bool]] = []
    for number, line in enumerate(stdout.splitlines(), 1):
        if not line.strip():
            # Blank separators carry no record; anything with content has to
            # be exactly the requested shape.
            continue
        fields = line.split("\t")
        if len(fields) != 2:
            raise ValueError(f"artifact listing line {number} is not two columns")
        name, expired = fields[0].strip(), fields[1].strip().lower()
        if not name:
            raise ValueError(f"artifact listing line {number} has no name")
        if expired not in ("true", "false"):
            raise ValueError(
                f"artifact listing line {number} has no boolean expiry: {expired!r}"
            )
        observed.append((name, expired == "true"))
    return tuple(observed)


def admit_evidence_artifact_v1(
    evidence_run_id: int,
    evidence_artifact: str,
    observer: object | None = None,
) -> corpus.ShardCorpusRejectedV1 | None:
    """Refuse a dispatch whose evidence run cannot carry what it names.

    Every lane downloads `evidence_artifact` from `evidence_run_id`; a run
    that does not exist, or carries no such artifact, turns one mistake into
    256 doomed jobs.  A positive integer is not evidence that a run exists,
    so the coordinator asks before it dispatches, and any failure to observe
    is itself a refusal — never a crash and never a silent proceed.
    """

    # Resolved here, not captured as a default: a default binds the module
    # attribute at definition time, which makes the injection point real for
    # a caller but invisible to anything that replaces the observer — the
    # seam would look injectable and not be.
    if observer is None:
        observer = gh_run_artifacts_v1
    try:
        observed = tuple(observer(evidence_run_id))  # type: ignore[operator]
        # An expired artifact is still listed but can no longer be
        # downloaded, so naming it would admit a stale evidence run and
        # reproduce the very failure this admission exists to prevent.  The
        # rule is applied here, not in the query, so a test can prove it.
        names = tuple(name for name, expired in observed if not expired)
    except Exception as error:
        # The observation is a hostile boundary: an unreachable run, a
        # missing token or a malformed reply must all land as one refusal.
        # The cause travels with it — an operator has to tell "no such run"
        # from "no token", and a bare refusal makes those look identical.
        # CalledProcessError.__repr__ drops stderr, and stderr is where the
        # only distinguishing text lives: "Not Found" against "no token"
        # against a network failure. Without it the operator debugs the wrong
        # axis, which is exactly what the refusal is supposed to prevent.
        cause = getattr(error, "stderr", None) or repr(error)
        return corpus._reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            f"verification dispatch cannot observe run {evidence_run_id}:"
            f" {str(cause).strip()}",
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
    # Printing is the default and dispatching is the opt-in.  The incident
    # that made this coordinator dangerous was a forgotten `--dry-run`: a
    # polarity where the safe run is the one you have to remember turns one
    # missing flag into hundreds of runs.  `--expect-lanes` makes the operator
    # state the scale before it happens, so a mistyped width is refused
    # instead of dispatched — swapping the two widths is a realistic slip and
    # silently means tens of thousands of jobs.
    parser.add_argument("--execute", action="store_true")
    parser.add_argument("--expect-lanes", type=int, default=None)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    if args.execute and args.mode == "plan":
        # A flag with live semantics must never be silently ignored: an
        # operator who believes a campaign started is worse off than one who
        # is told it did not.
        print("plan mode cannot dispatch: drop --execute", file=sys.stderr)
        return 64
    dispatching = args.execute

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
    else:
        commands = dispatch_commands_v1(plan, args.shard_width)
    if type(commands) is not tuple:
        # A rejected command set is a typed refusal, not an iterable: letting
        # it reach the loop below turns the module's own contract into a
        # TypeError at the boundary it is supposed to guard.
        print(f"lane dispatch rejected: {commands!r}", file=sys.stderr)
        return 64
    if not dispatching:
        for command in commands:
            print(" ".join(command))
        return 0
    if args.expect_lanes is None:
        # Its own refusal, not a comparison against `None`: the operator who
        # forgot the flag has to read what is missing, not a mismatch.
        print(
            "dispatch refused: --execute requires --expect-lanes",
            file=sys.stderr,
        )
        return 64
    if args.expect_lanes != len(commands):
        # The operator names the scale and reality has to agree.  A width
        # typed one token wrong produces a different campaign, and this is
        # the cheap moment to notice — before anything is observed or run.
        print(
            f"dispatch refused: --expect-lanes={args.expect_lanes} but the plan"
            f" has {len(commands)} lanes",
            file=sys.stderr,
        )
        return 64
    if args.mode == "verification-dispatch":
        # Fail closed before the first dispatch, and only here: the printing
        # path stays offline by contract, and this observation costs an
        # authenticated call that a mistyped width should never spend.
        refusal = admit_evidence_artifact_v1(
            args.evidence_run_id, args.evidence_artifact
        )
        if refusal is not None:
            print(f"verification dispatch refused: {refusal.detail}", file=sys.stderr)
            return 64
    print(f"dispatching {len(commands)} lanes", file=sys.stderr)
    launched = 0
    for command in commands:
        try:
            subprocess.run(command, check=True)
        except OSError as error:
            # Not only CalledProcessError: a missing `gh`, an exhausted file
            # descriptor or a killed child all abandon a campaign mid-flight,
            # and all of them must leave the same resumable report.
            print(
                f"dispatch stopped after {launched} of {len(commands)} lanes:"
                f" {str(error).strip()}",
                file=sys.stderr,
            )
            return 64
        except subprocess.CalledProcessError as error:
            # A campaign that dies mid-flight leaves runs already created.
            # Reporting where it stopped is what makes the retry resumable
            # instead of a duplicate of everything already dispatched.  The
            # child's stderr is not captured here on purpose — it belongs on
            # the operator's terminal — so only the exit status is quotable.
            print(
                f"dispatch stopped after {launched} of {len(commands)} lanes:"
                f" {str(error).strip()}",
                file=sys.stderr,
            )
            return 64
        launched += 1
        print(" ".join(command))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
