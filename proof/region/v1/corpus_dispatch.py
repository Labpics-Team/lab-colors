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

The verification lanes replay evidence produced by another run, so that run
is admitted before the first dispatch: which workflow produced it, what
triggered it, whether it succeeded, and which commit it stands on — the last
of these cannot be decided here and is reported to the operator instead.

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
import tempfile
from collections.abc import Iterable
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
# What every lane reads out of the artifact it downloads, relative to the
# artifact root.  This is a contract with `verification-lanes.yml`, which
# guards exactly these paths before it replays anything; a test binds the
# two together in both directions so they cannot drift apart.
EVIDENCE_LANE_INPUTS_V1 = (
    "job.bin",
    "comparator-bundle/comparator-manifest-v2.bin",
)
# One download of one artifact: generous for a slow network, far short of a
# wait an operator would mistake for work in progress.
OBSERVATION_TIMEOUT_SECONDS_V1 = 120
# An artifact name proves nothing about where the artifact came from.  Only a
# successful operator-triggered run of the producer workflow may be replayed:
# the path pins which workflow built the bundle, and the trigger pins whose
# code it was — a fork's pull request can run the same workflow and publish an
# artifact of exactly the allowlisted name.
EVIDENCE_WORKFLOW_PATH_V1 = ".github/workflows/full-domain-run.yml"
EVIDENCE_RUN_EVENT_V1 = "workflow_dispatch"
EVIDENCE_RUN_STATUS_V1 = "completed"
EVIDENCE_RUN_CONCLUSION_V1 = "success"
# A projection, never the whole run object: the reply carries fields this
# module has no reason to read, and printing a reply of unknown shape is how
# this project has leaked before.
RUN_PROVENANCE_JQ_V1 = "[.path, .event, .status, .conclusion, .head_sha] | @tsv"
COMMIT_SHA_LENGTH_V1 = 40
COMMIT_SHA_ALPHABET_V1 = frozenset("0123456789abcdef")
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
# One observation — a listing, a projection or a two-small-file download:
# generous for a slow network, far short of a wait an operator would mistake
# for work in progress.
OBSERVATION_TIMEOUT_SECONDS_V1 = 60


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
        # A hung observation is worse than a refused one: without a deadline
        # the coordinator waits forever on the one call standing between an
        # operator and 256 dispatches.  The timeout raises, and the admission
        # turns every observation failure into a typed refusal.
        timeout=OBSERVATION_TIMEOUT_SECONDS_V1,
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


@dataclass(frozen=True)
class RunProvenanceV1:
    """The run fields the admission reads, and nothing else.

    Where the evidence came from is four coordinates — which workflow built
    it, what triggered that workflow, whether it finished successfully, and
    which commit it stands on.  They travel as one record so no caller can
    check two of them and forget the rest.
    """

    path: str
    event: str
    status: str
    conclusion: str
    head_sha: str


def gh_run_provenance_v1(run_id: int) -> RunProvenanceV1:
    """The run's origin as GitHub reports it, projected to the named fields.

    The second impure boundary of this admission, and the same contract as
    the first: it observes and reports, deciding nothing.  The query names
    the fields instead of fetching the run object, because everything not
    named here is a field this module would carry without ever reading it.
    """

    completed = subprocess.run(
        (
            "gh",
            "api",
            f"repos/{{owner}}/{{repo}}/actions/runs/{run_id}",
            "--jq",
            RUN_PROVENANCE_JQ_V1,
        ),
        capture_output=True,
        text=True,
        check=True,
        # The first call of the whole campaign: a hung `gh` here is even
        # earlier than the artifact listing, and just as indistinguishable
        # from work in progress.
        timeout=OBSERVATION_TIMEOUT_SECONDS_V1,
    )
    return parse_run_provenance_v1(completed.stdout)


def parse_run_provenance_v1(stdout: str) -> RunProvenanceV1:
    """Decode one run's projected wire form into the observed record.

    Shape only: which values are acceptable is `admit_run_provenance_v1`,
    where a test can reach it.  What is refused here is a reply that is not
    exactly one five-column record — an empty or drifted reply read as a
    default would admit precisely the runs this observation exists to catch.
    """

    records = [line for line in stdout.splitlines() if line.strip()]
    if len(records) != 1:
        raise ValueError(
            f"run provenance is not one record: {len(records)} lines"
        )
    fields = records[0].split("\t")
    if len(fields) != 5:
        raise ValueError(
            f"run provenance record is not five columns: {len(fields)}"
        )
    path, event, status, conclusion, head_sha = (
        field.strip() for field in fields
    )
    return RunProvenanceV1(path, event, status, conclusion, head_sha)


def admit_run_provenance_v1(
    provenance: object,
) -> corpus.ShardCorpusRejectedV1 | None:
    """Which observed run may be replayed — the whole rule, and nothing impure.

    A run id and an artifact name say only that some run holds a file of the
    right name.  Three runs pass that and must not pass this: one produced by
    a different workflow, one a fork's pull request produced, and one that
    never finished successfully.  Each of them sends 256 lanes to replay
    something the operator did not intend.
    """

    if type(provenance) is not RunProvenanceV1:
        return corpus._reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            "evidence run provenance is not an observed record",
        )
    if provenance.path != EVIDENCE_WORKFLOW_PATH_V1:
        # The path, not the file name: a workflow of the same basename in a
        # foreign directory is a different producer.
        return corpus._reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            f"evidence run was produced by {provenance.path!r},"
            f" not {EVIDENCE_WORKFLOW_PATH_V1}",
        )
    if provenance.event != EVIDENCE_RUN_EVENT_V1:
        return corpus._reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            f"evidence run was triggered by {provenance.event!r},"
            f" not {EVIDENCE_RUN_EVENT_V1}",
        )
    if (
        provenance.status != EVIDENCE_RUN_STATUS_V1
        or provenance.conclusion != EVIDENCE_RUN_CONCLUSION_V1
    ):
        # Both halves: a run still in flight can already have uploaded one
        # engine's artifact, and a conclusion is only final once the status
        # says the run is.
        return corpus._reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            f"evidence run is {provenance.status!r}/{provenance.conclusion!r},"
            f" not {EVIDENCE_RUN_STATUS_V1}/{EVIDENCE_RUN_CONCLUSION_V1}",
        )
    if (
        len(provenance.head_sha) != COMMIT_SHA_LENGTH_V1
        or not COMMIT_SHA_ALPHABET_V1.issuperset(provenance.head_sha)
    ):
        # The commit is what the operator checks the campaign against, so a
        # run that carries no readable one cannot be admitted for being green:
        # an abbreviated or absent sha is not something to paste into `git`.
        return corpus._reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            f"evidence run carries no commit sha: {provenance.head_sha!r}",
        )
    return None


def admit_evidence_run_v1(
    evidence_run_id: int,
    observer: object | None = None,
) -> corpus.ShardCorpusRejectedV1 | None:
    """Refuse a dispatch whose evidence run is not the one the campaign means.

    Observation and reporting only; the rule is `admit_run_provenance_v1`.
    Any failure to observe is a refusal, never a crash and never a silent
    proceed, on the same reasoning as the artifact admission.

    An admitted run's commit goes to the operator here.  Nothing in this
    process can tell last week's green producer run from this week's — the
    rules above admit both — so the one coordinate that decides it is put in
    front of the operator at the moment of admission, while 256 lanes have
    still not started.
    """

    if observer is None:
        observer = gh_run_provenance_v1
    try:
        provenance = observer(evidence_run_id)  # type: ignore[operator]
    except Exception as error:
        # Same hostile boundary, same reason to carry the cause: "no such
        # run" and "no token" are indistinguishable without stderr, and
        # `CalledProcessError.__repr__` drops it.
        cause = getattr(error, "stderr", None) or repr(error)
        return corpus._reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            f"verification dispatch cannot observe run {evidence_run_id}:"
            f" {str(cause).strip()}",
        )
    refusal = admit_run_provenance_v1(provenance)
    if refusal is not None:
        return refusal
    print(
        f"evidence run {evidence_run_id} admitted:"
        f" head_sha={provenance.head_sha}",
        file=sys.stderr,
    )
    return None


def admit_evidence_artifact_v1(    evidence_run_id: int,
    evidence_artifact: str,
    observer: object | None = None,
) -> corpus.ShardCorpusRejectedV1 | None:
    """Refuse a dispatch whose evidence run cannot carry what it names.

    Every lane downloads `evidence_artifact` from `evidence_run_id`; a run
    that does not exist, or carries no such artifact, turns one mistake into
    256 doomed jobs.  A positive integer is not evidence that a run exists,
    so the coordinator asks before it dispatches, and any failure to observe
    is itself a refusal — never a crash and never a silent proceed.    """

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
    in `match_lane_runs_v1` where a test can reach it.    """

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
            ".[] | [.databaseId, .displayTitle, .conclusion] | @tsv",        ),
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
        # workflow" from "no token" from a network failure.  `stderr` is
        # preferred over `repr` for legibility — the text itself rather than
        # the text wrapped in a constructor call.
        cause = getattr(error, "stderr", None) or repr(error)
        return _collection_reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            f"lane run collection cannot observe {VERIFICATION_WORKFLOW_V1}:"
            f" {str(cause).strip()}",
        )
    matched = match_lane_runs_v1(plan, evidence_run_id, evidence_artifact, observed)
    if (
        type(matched) is LaneRunCollectionRejectedV1
        and matched.reason is corpus.ShardCorpusReasonV1.INCOMPLETE_COVER
        and len(observed) == limit
    ):
        # A listing saturated at the limit truncates the oldest runs, so the
        # gap may be the query's, not the campaign's.  Blaming the campaign
        # sends the operator to re-dispatch 256 lanes when the fix is one
        # flag — the exact misdirection this admission exists to prevent.
        return _collection_reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            f"the listing is saturated at --run-limit {limit}, so older lane"
            f" runs may lie beyond it; raise the limit before blaming the"
            f" campaign ({matched.detail})",
            matched.missing,
            matched.duplicated,
        )
    return matched


def missing_lane_inputs_v1(observed_paths: Iterable[str]) -> tuple[str, ...]:
    """Which lane inputs a downloaded evidence artifact does not carry.

    The rule of this admission, kept pure and off the network so a test can
    reach it.  Paths match exactly, in the declared order: the lane runs
    `test -f` on the exact path, so a suffixed sibling, the directory above
    it, a backslash-joined spelling or a different case is absent to the
    lane and must read absent here.  Entries that are not strings cannot be
    a path the lane found, so they never satisfy a requirement.
    """

    present = frozenset(path for path in observed_paths if type(path) is str)
    return tuple(
        required
        for required in EVIDENCE_LANE_INPUTS_V1
        if required not in present
    )


def gh_evidence_artifact_paths_v1(run_id: int, artifact: str) -> tuple[str, ...]:
    """Every file the named artifact of that run carries, relative to its root.

    The one impure boundary of this admission: it downloads exactly what the
    lanes will download, into a temporary directory it owns and always
    removes, and reports what arrived without deciding anything.  Which of
    those paths count is a rule, so it lives in `missing_lane_inputs_v1` —
    filtering here would put the rule behind the network.  The evidence is
    two files well under a megabyte, so this one download costs less than a
    single doomed lane, let alone 256 of them.
    """

    with tempfile.TemporaryDirectory(prefix="verification-evidence-") as tmp:
        root = Path(tmp)
        subprocess.run(
            (
                "gh",
                "run",
                "download",
                str(run_id),
                "--name",
                artifact,
                "--dir",
                str(root),
            ),
            capture_output=True,
            text=True,
            check=True,
            # A hung download is worse than a refused one: without a deadline
            # the coordinator waits forever on the last check standing between
            # an operator and 256 dispatches.
            timeout=OBSERVATION_TIMEOUT_SECONDS_V1,
        )
        return tuple(
            sorted(
                path.relative_to(root).as_posix()
                for path in root.rglob("*")
                if path.is_file()
            )
        )


def admit_evidence_artifact_content_v1(    evidence_run_id: int,
    evidence_artifact: str,
    observer: object | None = None,
) -> corpus.ShardCorpusRejectedV1 | None:
    """Refuse a dispatch whose evidence artifact lacks what the lanes read.

    A name is not a layout.  An evidence run produced before the exporter
    settled on today's `evidence-out/` carries the right artifact name and
    the wrong contents: it passes every name check and then dies in all 256
    lanes, seconds apart, on the first `test -f`.  So the coordinator reads
    the artifact once before it dispatches, and admits it only when every
    lane input is there.  Any failure to observe is itself a refusal —
    never a crash, never a silent proceed.  The run id and the artifact
    name are admitted upstream; this admission speaks about content only.    """

    # Resolved here, not captured as a default: a default binds the module
    # attribute at definition time, which makes the injection point real for
    # a caller but invisible to anything that replaces the observer — the
    # seam would look injectable and not be.
    if observer is None:
        observer = gh_evidence_artifact_paths_v1
    try:
        observed = tuple(observer(evidence_run_id, evidence_artifact))  # type: ignore[operator]
    except Exception as error:
        # The download is a hostile boundary: a deleted run, an expired
        # artifact, a missing token and a broken network must all land as
        # one refusal.  The cause travels with it — an operator has to tell
        # "no such run" from "no token", and a bare refusal makes those look
        # identical.  `stderr` is preferred over `repr` because it is the text
        # itself rather than the text wrapped in a constructor call; `repr`
        # does carry it, so this is legibility, not recovery.
        cause = getattr(error, "stderr", None) or repr(error)
        return corpus._reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            f"verification dispatch cannot read artifact {evidence_artifact!r}"
            f" of run {evidence_run_id}: {str(cause).strip()}",
        )
    missing = missing_lane_inputs_v1(observed)
    if missing:
        return corpus._reject(
            corpus.ShardCorpusReasonV1.FOREIGN_INPUT,
            f"artifact {evidence_artifact!r} of run {evidence_run_id} carries"
            f" no {', '.join(missing)}",        )
    return None

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
    # Printing is the default and dispatching is the opt-in.  The incident
    # that made this coordinator dangerous was a forgotten `--dry-run`: a
    # polarity where the safe run is the one you have to remember turns one
    # missing flag into hundreds of runs.  `--expect-lanes` makes the operator
    # state the scale before it happens, so a mistyped width is refused
    # instead of dispatched — swapping the two widths is a realistic slip and
    # silently means tens of thousands of jobs.
    parser.add_argument("--execute", action="store_true")
    parser.add_argument("--expect-lanes", type=int, default=None)
    parser.add_argument("--run-limit", type=int, default=LANE_RUN_QUERY_LIMIT_V1)
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

    if args.mode == "collect":
        # Every one of these coordinates is spent on a network query, so all
        # three are judged before it is made.  A query fired with a foreign
        # coordinate comes back as a refusal about `verification-lanes.yml` or
        # as an empty listing — the first blames the workflow for the
        # operator's typo, the second reports a 256-window hole in a campaign
        # that may be intact.  Each refusal names the argument at fault,
        # because the exit code cannot: it is 64 for all of them.
        if args.evidence_run_id is None or args.evidence_run_id <= 0:
            print(
                "lane run collection requires a positive --evidence-run-id",
                file=sys.stderr,
            )
            return 64
        if args.evidence_artifact not in EVIDENCE_ARTIFACTS_V1:
            print(
                "lane run collection requires --evidence-artifact from "
                + ", ".join(EVIDENCE_ARTIFACTS_V1),
                file=sys.stderr,
            )
            return 64
        if args.run_limit <= 0:
            print(
                "lane run collection requires a positive --run-limit",
                file=sys.stderr,
            )
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
    if type(commands) is not tuple:
        # A rejected command set is a typed refusal, not an iterable.  Letting
        # it reach the loop turns this module's own contract into a TypeError
        # at the boundary it exists to guard — and a mistyped artifact name is
        # the likeliest way an operator gets here.
        print(f"lane dispatch rejected: {commands.detail}", file=sys.stderr)
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
        # path stays offline by contract, and these observations cost
        # authenticated calls that a mistyped width should never spend.
        # Origin before contents: a run of the wrong workflow, the wrong
        # trigger or the wrong outcome lists an artifact of exactly the right
        # name, so asking about the artifact first would clear a run that
        # should never have been considered.
        refusal = admit_evidence_run_v1(args.evidence_run_id)
        if refusal is not None:
            print(f"verification dispatch refused: {refusal.detail}", file=sys.stderr)
            return 64
        refusal = admit_evidence_artifact_v1(
            args.evidence_run_id, args.evidence_artifact
        )
        if refusal is not None:
            print(f"verification dispatch refused: {refusal.detail}", file=sys.stderr)
            return 64
        # Contents last: this one downloads, and the free refusals above must
        # never cost a fetch.  A name is not a layout — an older producer's
        # run passes every check before this and dies in all 256 lanes.
        refusal = admit_evidence_artifact_content_v1(
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
        except (OSError, subprocess.CalledProcessError) as error:
            # A campaign that dies mid-flight leaves runs already created, and
            # every way it can die — a nonzero `gh`, a missing binary, an
            # exhausted descriptor, a killed child — must leave the same
            # resumable report: without the count a retry duplicates
            # everything already dispatched.  The child's stderr is not
            # captured on purpose — it belongs on the operator's terminal —
            # so only the exit status is quotable.
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
