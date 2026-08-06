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

`gh workflow run` returns no run id, so a dispatched campaign is only a set
of names.  The collect mode closes that gap: `verification-lanes.yml` titles
every lane run after its own coordinates, so the campaign's run ids are a
query against those titles instead of a guess about creation times — which is
exactly what breaks when two campaigns overlap in time.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from dataclasses import dataclass
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
LANE_RUN_COLLECTION_SCHEMA_V1 = "corpus-lane-runs-v1"
LANE_RUN_COLLECTION_NAME_V1 = "lane-runs.json"
# Only a successful lane produced the fragment the dual proof will replay, so
# a run in any other terminal state — and an unfinished one, whose conclusion
# is still empty — leaves its window uncovered.
LANE_RUN_SUCCESS_CONCLUSION_V1 = "success"
# A full-domain cover is 512 runs (256 per engine) before a single rerun, and
# a truncated listing can only hide runs, which surfaces as missing windows —
# a loud refusal, never a short list.
LANE_RUN_QUERY_LIMIT_V1 = 2000
# The run-name renders ordinals through GitHub's expression syntax, which
# emits plain decimals.  `int()` also accepts `+7`, `007`, `1_0` and non-ASCII
# digits, and each of those would let a foreign title claim a plan window, so
# the parser admits exactly the canonical rendering.
_CANONICAL_ORDINAL_V1 = re.compile(r"(?:0|[1-9][0-9]*)")
# A refusal's prose stays readable at 256 windows; the machine-readable set
# travels in the refusal's own fields, so nothing is lost by truncating here.
_SUMMARY_WINDOWS_V1 = 8
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


@dataclass(frozen=True)
class LaneRunObservationV1:
    """Whitelisted fields of one workflow run, exactly as GitHub reported them.

    Three fields are the whole observation surface: the id the dual proof
    needs, the title that carries the lane coordinates, and the conclusion
    that says whether the lane produced anything.  Nothing else from the API
    reply is admitted, so no unreviewed field can reach a decision or a log.
    """

    run_id: int
    display_title: str
    conclusion: str

    def __post_init__(self) -> None:
        if (
            type(self.run_id) is not int
            or self.run_id <= 0
            or type(self.display_title) is not str
            or type(self.conclusion) is not str
        ):
            raise TypeError("invalid lane run observation")


@dataclass(frozen=True)
class LaneRunNameV1:
    """The coordinates one lane run title carries."""

    evidence_artifact: str
    window_start: int
    window_points: int
    evidence_run_id: int

    def __post_init__(self) -> None:
        if (
            type(self.evidence_artifact) is not str
            or not self.evidence_artifact
            or type(self.window_start) is not int
            or self.window_start < 0
            or type(self.window_points) is not int
            or self.window_points < 1
            or type(self.evidence_run_id) is not int
            or self.evidence_run_id < 1
        ):
            raise TypeError("invalid lane run name")


@dataclass(frozen=True)
class LaneRunCollectionV1:
    """One successful lane run per plan window, in plan order."""

    evidence_run_id: int
    evidence_artifact: str
    lanes: tuple[tuple[int, int, int], ...]

    def __post_init__(self) -> None:
        if (
            type(self.evidence_run_id) is not int
            or self.evidence_run_id < 1
            or type(self.evidence_artifact) is not str
            or self.evidence_artifact not in EVIDENCE_ARTIFACTS_V1
            or type(self.lanes) is not tuple
            or not self.lanes
            or any(
                type(lane) is not tuple
                or len(lane) != 3
                or any(type(value) is not int for value in lane)
                for lane in self.lanes
            )
        ):
            raise TypeError("invalid lane run collection")


@dataclass(frozen=True)
class LaneRunCollectionRejectedV1:
    """Why no list of lane run ids exists.

    A partial cover is the failure this mode is built to catch, so the windows
    that have no run and the windows that have more than one travel with the
    refusal as data: the operator re-dispatches exactly those, and the caller
    never has to parse prose to learn what is missing.
    """

    reason: corpus.ShardCorpusReasonV1
    detail: str
    missing: tuple[tuple[int, int], ...] = ()
    duplicated: tuple[tuple[int, int], ...] = ()

    def __post_init__(self) -> None:
        if type(self.reason) is not corpus.ShardCorpusReasonV1:
            raise TypeError("invalid lane run collection rejection reason")
        if type(self.detail) is not str or not self.detail:
            raise TypeError("invalid lane run collection rejection detail")
        for windows in (self.missing, self.duplicated):
            if type(windows) is not tuple or any(
                type(window) is not tuple
                or len(window) != 2
                or any(type(value) is not int for value in window)
                for window in windows
            ):
                raise TypeError("invalid lane run collection window set")


def _collection_reject(
    reason: corpus.ShardCorpusReasonV1,
    detail: str,
    missing: tuple[tuple[int, int], ...] = (),
    duplicated: tuple[tuple[int, int], ...] = (),
) -> LaneRunCollectionRejectedV1:
    return LaneRunCollectionRejectedV1(reason, detail, missing, duplicated)


def render_windows_v1(windows: tuple[tuple[int, int], ...]) -> str:
    """The wire rendering of a window set, in the run-name's own notation."""

    return ",".join(f"{start}+{points}" for start, points in windows) or "none"


def _window_summary_v1(label: str, windows: tuple[tuple[int, int], ...]) -> str:
    """One bounded human line; the refusal itself carries the full set."""

    head = render_windows_v1(windows[:_SUMMARY_WINDOWS_V1])
    if len(windows) <= _SUMMARY_WINDOWS_V1:
        return f"{label}={head}"
    return f"{label}={head} (+{len(windows) - _SUMMARY_WINDOWS_V1} more)"


def _ordinal_v1(token: str, minimum: int) -> int | None:
    if _CANONICAL_ORDINAL_V1.fullmatch(token) is None:
        return None
    value = int(token)
    return value if value >= minimum else None


def parse_lane_run_name_v1(display_title: object) -> LaneRunNameV1 | None:
    """The lane coordinates a run title carries, or None if it carries none.

    `verification-lanes.yml` titles every lane run

        lane <evidence_artifact> <window_start>+<window_points> of <evidence_run_id>

    from a folded scalar, which collapses to exactly single-space separation.
    Anything else — another workflow's run, another naming scheme, a title
    that merely resembles the form — is not a lane run of this campaign, and
    returning None keeps that judgement out of the network boundary.
    """

    if type(display_title) is not str:
        return None
    tokens = display_title.split(" ")
    if len(tokens) != 5 or tokens[0] != "lane" or tokens[3] != "of":
        return None
    artifact, window, run_id_token = tokens[1], tokens[2], tokens[4]
    if not artifact:
        return None
    start_token, separator, points_token = window.partition("+")
    if not separator:
        return None
    window_start = _ordinal_v1(start_token, 0)
    window_points = _ordinal_v1(points_token, 1)
    evidence_run_id = _ordinal_v1(run_id_token, 1)
    if window_start is None or window_points is None or evidence_run_id is None:
        return None
    return LaneRunNameV1(artifact, window_start, window_points, evidence_run_id)


def _canonical_plan_v1(plan: object) -> bool:
    """Windows a run can be matched against one at a time.

    Overlap is the one thing that must not pass: two windows sharing an
    ordinal would make "exactly one run per window" undefined before any run
    is even observed.  Whether the windows also cover the domain exactly is
    `lane_plan_v1`'s judgement, not this one's.
    """

    if type(plan) is not tuple or not plan:
        return False
    cursor = 0
    for window in plan:
        if (
            type(window) is not tuple
            or len(window) != 2
            or any(type(value) is not int for value in window)
            or window[0] < cursor
            or window[1] < 1
        ):
            return False
        cursor = window[0] + window[1]
    return True


def match_lane_runs_v1(
    plan: object,
    evidence_run_id: object,
    evidence_artifact: object,
    observations: object,
) -> LaneRunCollectionV1 | LaneRunCollectionRejectedV1:
    """Bind observed run titles to the plan windows, or refuse with the gaps.

    Pure: it decides only from the plan and the observation it is handed, so
    every rule below — which campaign a title belongs to, which engine, which
    conclusion counts, what a complete cover is — is reachable by a test
    without a network.  A window with no successful run and a window with two
    are the same class of failure: the campaign's run ids are not yet a fact,
    and a list that hid either would let the dual proof rest on a cover
    nobody checked.
    """

    if not _canonical_plan_v1(plan):
        return _collection_reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            "lane run collection requires a canonical lane plan",
        )
    if type(evidence_run_id) is not int or evidence_run_id <= 0:
        return _collection_reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            "lane run collection requires a positive evidence run id",
        )
    if (
        type(evidence_artifact) is not str
        or evidence_artifact not in EVIDENCE_ARTIFACTS_V1
    ):
        return _collection_reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            "lane run collection requires an allowlisted engine artifact",
        )
    try:
        observed = tuple(observations)  # type: ignore[arg-type]
    except Exception:
        # The observation is a hostile boundary: any iterator failure — not
        # just a non-iterable — must land as the typed refusal.
        return _collection_reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            "lane run collection requires an iterable observation",
        )
    if any(type(seen) is not LaneRunObservationV1 for seen in observed):
        return _collection_reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            "lane run collection requires canonical lane run observations",
        )

    covering: dict[tuple[int, int], list[int]] = {}
    for seen in observed:
        if seen.conclusion != LANE_RUN_SUCCESS_CONCLUSION_V1:
            continue
        name = parse_lane_run_name_v1(seen.display_title)
        if name is None:
            continue
        # Two campaigns overlap in time by design — the two engines replay the
        # same windows of the same evidence run — so the artifact and the
        # evidence run id, not the clock, are what separate them.
        if (
            name.evidence_run_id != evidence_run_id
            or name.evidence_artifact != evidence_artifact
        ):
            continue
        run_ids = covering.setdefault((name.window_start, name.window_points), [])
        # One run listed twice is still one run: deduplicating by id keeps a
        # repetitive listing from manufacturing a duplicate-cover refusal.
        if seen.run_id not in run_ids:
            run_ids.append(seen.run_id)

    missing = tuple(window for window in plan if not covering.get(window))
    duplicated = tuple(
        window for window in plan if len(covering.get(window, ())) > 1
    )
    if missing or duplicated:
        return _collection_reject(
            corpus.ShardCorpusReasonV1.INCOMPLETE_COVER,
            "the lane run cover is incomplete: "
            f"{_window_summary_v1('missing', missing)} "
            f"{_window_summary_v1('duplicated', duplicated)}",
            missing,
            duplicated,
        )
    return LaneRunCollectionV1(
        evidence_run_id,
        evidence_artifact,
        tuple(
            (start, points, covering[(start, points)][0]) for start, points in plan
        ),
    )


def lane_runs_json_v1(
    collection: object,
) -> str | LaneRunCollectionRejectedV1:
    """The deterministic wire form of one collected campaign."""

    if type(collection) is not LaneRunCollectionV1:
        return _collection_reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            "the lane run wire form requires a collected campaign",
        )
    return json.dumps(
        {
            "schema": LANE_RUN_COLLECTION_SCHEMA_V1,
            "evidence_run_id": collection.evidence_run_id,
            "evidence_artifact": collection.evidence_artifact,
            "lane_count": len(collection.lanes),
            "lanes": [
                {
                    "window_start": start,
                    "window_points": points,
                    "run_id": run_id,
                }
                for start, points, run_id in collection.lanes
            ],
        },
        sort_keys=True,
        separators=(",", ":"),
    )


def gh_lane_runs_v1(limit: int = LANE_RUN_QUERY_LIMIT_V1) -> tuple[
    LaneRunObservationV1, ...
]:
    """Every verification lane run GitHub lists, as whitelisted observations.

    The one impure boundary of the collection: it observes and reports what
    it saw, without deciding anything.  The projection is done by `--jq` in
    the query itself, so no unreviewed field of the API reply ever enters the
    process; which of these runs belongs to a campaign is a rule, and it lives
    in `match_lane_runs_v1` where a test can reach it.
    """

    completed = subprocess.run(
        (
            "gh",
            "run",
            "list",
            "--workflow",
            VERIFICATION_WORKFLOW_V1,
            "--limit",
            str(limit),
            "--json",
            "databaseId,displayTitle,conclusion",
            "--jq",
            ".[] | [.databaseId, .displayTitle, .conclusion] | @tsv",
        ),
        capture_output=True,
        text=True,
        check=True,
    )
    observed: list[LaneRunObservationV1] = []
    for line in completed.stdout.splitlines():
        if not line:
            continue
        fields = line.split("\t")
        if len(fields) != 3:
            # A title carrying a tab would shift every field after it, so a
            # record that does not project exactly is refused, not guessed.
            raise ValueError("gh listed a lane run in an unreadable shape")
        run_id, display_title, conclusion = fields
        observed.append(
            LaneRunObservationV1(int(run_id), display_title, conclusion)
        )
    return tuple(observed)


def collect_lane_runs_v1(
    plan: object,
    evidence_run_id: object,
    evidence_artifact: object,
    observer: object | None = None,
    limit: int = LANE_RUN_QUERY_LIMIT_V1,
) -> LaneRunCollectionV1 | LaneRunCollectionRejectedV1:
    """Observe the lane runs once, then match them against the plan.

    The seam between the two is where a failure to observe becomes a typed
    refusal instead of a crash — an unreachable API, a missing token or an
    unreadable reply must not look like an empty campaign, because an empty
    campaign and a broken query lead an operator to opposite actions.
    """

    # Resolved here, not captured as a default: a default binds the module
    # attribute at definition time, which would make the injection point look
    # real to a caller that replaces the observer and quietly not be.
    if observer is None:
        observer = gh_lane_runs_v1
    try:
        observed = tuple(observer(limit))  # type: ignore[operator]
    except Exception as error:
        # The cause travels with the refusal: an operator has to tell "no such
        # workflow" from "no token" from a network failure, and
        # CalledProcessError.__repr__ drops stderr — the only place that text
        # lives.
        cause = getattr(error, "stderr", None) or repr(error)
        return _collection_reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            f"lane run collection cannot observe {VERIFICATION_WORKFLOW_V1}:"
            f" {str(cause).strip()}",
        )
    return match_lane_runs_v1(plan, evidence_run_id, evidence_artifact, observed)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--mode",
        choices=("plan", "dispatch", "verification-dispatch", "collect"),
        required=True,
    )
    parser.add_argument("--lane-width", type=int, default=DEFAULT_LANE_WIDTH)
    parser.add_argument("--shard-width", type=int, default=DEFAULT_SHARD_WIDTH)
    parser.add_argument("--evidence-run-id", type=int, default=None)
    parser.add_argument("--evidence-artifact", type=str, default=None)
    parser.add_argument("--run-limit", type=int, default=LANE_RUN_QUERY_LIMIT_V1)
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

    if args.mode == "collect":
        if args.evidence_run_id is None:
            print("lane run collection requires --evidence-run-id", file=sys.stderr)
            return 64
        if args.evidence_artifact is None:
            print("lane run collection requires --evidence-artifact", file=sys.stderr)
            return 64
        collection = collect_lane_runs_v1(
            plan,
            args.evidence_run_id,
            args.evidence_artifact,
            limit=args.run_limit,
        )
        if type(collection) is not LaneRunCollectionV1:
            # Nothing is written: a partial list of run ids reads exactly like
            # a complete one to whatever consumes it next.
            print(f"lane run collection refused: {collection.detail}", file=sys.stderr)
            for label, windows in (
                ("missing", collection.missing),
                ("duplicated", collection.duplicated),
            ):
                if windows:
                    print(
                        f"{label} windows ({len(windows)}):"
                        f" {render_windows_v1(windows)}",
                        file=sys.stderr,
                    )
            return 64
        args.out.mkdir(parents=True, exist_ok=True)
        (args.out / LANE_RUN_COLLECTION_NAME_V1).write_bytes(
            lane_runs_json_v1(collection).encode("ascii")
        )
        print(
            f"collected lanes={len(collection.lanes)} "
            f"artifact={collection.evidence_artifact} "
            f"evidence_run={collection.evidence_run_id}"
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
