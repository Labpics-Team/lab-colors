#!/usr/bin/env python3
"""Fail-closed admission for the exact PR/merge-queue workflow set."""

from __future__ import annotations

from dataclasses import dataclass
import json
import os
import sys
import time
from typing import Iterable, Mapping
import urllib.parse
import urllib.request


REQUIRED_WORKFLOWS = frozenset(
    {
        ".github/workflows/ci.yml",
        ".github/workflows/native-conformance.yml",
    }
)
GATE_WORKFLOW = ".github/workflows/ci-gate.yml"
PENDING_STATUSES = frozenset(
    {"queued", "in_progress", "waiting", "requested", "pending"}
)


@dataclass(frozen=True)
class GateDecision:
    state: str
    reasons: tuple[str, ...]


def _run_order(run: Mapping[str, object]) -> tuple[int, int]:
    attempt = run.get("run_attempt")
    run_id = run.get("id")
    return (
        run_id if type(run_id) is int else -1,
        attempt if type(attempt) is int else -1,
    )


def evaluate_runs(
    runs: Iterable[Mapping[str, object]],
    *,
    head_sha: str,
    event: str,
) -> GateDecision:
    """Classify only exact-event, exact-head runs; unknown peers fail closed."""
    if event not in {"pull_request", "merge_group"}:
        return GateDecision("fail", (f"unsupported gate event: {event!r}",))
    if not head_sha:
        return GateDecision("fail", ("empty head SHA",))

    candidates: dict[str, list[Mapping[str, object]]] = {
        path: [] for path in REQUIRED_WORKFLOWS
    }
    unexpected: set[str] = set()
    for run in runs:
        if run.get("head_sha") != head_sha or run.get("event") != event:
            continue
        path = run.get("path")
        if not isinstance(path, str):
            unexpected.add(repr(path))
        elif path == GATE_WORKFLOW:
            continue
        elif path in candidates:
            candidates[path].append(run)
        else:
            unexpected.add(path)

    if unexpected:
        return GateDecision(
            "fail",
            tuple(f"unadmitted peer workflow: {path}" for path in sorted(unexpected)),
        )

    missing: list[str] = []
    pending: list[str] = []
    failed: list[str] = []
    for path in sorted(REQUIRED_WORKFLOWS):
        matching = candidates[path]
        if not matching:
            missing.append(f"missing required workflow: {path}")
            continue
        latest = max(matching, key=_run_order)
        status = latest.get("status")
        conclusion = latest.get("conclusion")
        if status in PENDING_STATUSES:
            pending.append(f"pending required workflow: {path} ({status})")
        elif status != "completed":
            failed.append(f"invalid workflow status: {path} ({status!r})")
        elif conclusion != "success":
            failed.append(f"required workflow is not successful: {path} ({conclusion!r})")

    if failed:
        return GateDecision("fail", tuple(failed))
    if missing or pending:
        return GateDecision("wait", tuple(missing + pending))
    return GateDecision("success", ("all required workflows completed successfully",))


def _workflow_runs(*, token: str, repo: str, sha: str, event: str) -> list[dict]:
    query = urllib.parse.urlencode(
        {"head_sha": sha, "event": event, "per_page": "100"}
    )
    url = f"https://api.github.com/repos/{repo}/actions/runs?{query}"
    request = urllib.request.Request(
        url,
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "labcolors-ci-peer-gate",
        },
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        payload = json.load(response)
    runs = payload.get("workflow_runs")
    if not isinstance(runs, list):
        raise RuntimeError("GitHub workflow-runs response has no array")
    if len(runs) >= 100:
        raise RuntimeError("workflow-runs result reached the unpaginated safety bound")
    if not all(isinstance(run, dict) for run in runs):
        raise RuntimeError("GitHub workflow-runs response contains a non-object")
    return runs


def main() -> int:
    token = os.environ.get("GH_TOKEN", "")
    repo = os.environ.get("REPO", "")
    sha = os.environ.get("SHA", "")
    event = os.environ.get("GATE_EVENT", "")
    if not token or not repo or not sha or not event:
        print("ci peer gate: required environment is incomplete", file=sys.stderr)
        return 2

    started = time.monotonic()
    timeout_seconds = 85 * 60
    while True:
        try:
            runs = _workflow_runs(token=token, repo=repo, sha=sha, event=event)
            decision = evaluate_runs(runs, head_sha=sha, event=event)
        except Exception as error:  # boundary: API/JSON failures are typed as gate failure
            print(f"ci peer gate: evidence retrieval failed: {error}", file=sys.stderr)
            return 1
        elapsed = int(time.monotonic() - started)
        print(f"ci peer gate: {decision.state} after {elapsed}s", flush=True)
        for reason in decision.reasons:
            print(f" - {reason}", flush=True)
        if decision.state == "success":
            return 0
        if decision.state == "fail":
            return 1
        if elapsed >= timeout_seconds:
            print("ci peer gate: timed out waiting for required workflows", file=sys.stderr)
            return 1
        time.sleep(15)


if __name__ == "__main__":
    raise SystemExit(main())
