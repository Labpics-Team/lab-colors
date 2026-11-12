#!/usr/bin/env python3
"""Verify the native CI dependency graph and its compact GitHub results."""

from __future__ import annotations

from dataclasses import dataclass
import json
import os
from pathlib import Path
import re
import sys

import yaml


ROOT = Path(__file__).resolve().parents[2]
ENTRY = ".github/workflows/ci-gate.yml"
COMPLETE_MODE = "complete"
ENTRYPOINT = Path(__file__).resolve().relative_to(ROOT).as_posix()
COMPLETE_COMMAND = f"python3 {ENTRYPOINT} {COMPLETE_MODE}"
CHECKS = "${{ format('{{\"results\":{0},\"children\":{1}}}', toJSON(jobs.*.result), toJSON(jobs.*.outputs.checks)) }}"
QUALIFICATION = "${{ always() && github.event_name != 'pull_request' }}"
EVENTS = {"pull_request", "merge_group"}


class WorkflowError(AssertionError):
    pass


@dataclass(frozen=True)
class Producer:
    result: str
    checks: tuple | None


class WorkflowLoader(yaml.BaseLoader):
    """GitHub keys remain strings; ambiguous duplicate keys are rejected."""

    def construct_mapping(self, node, deep=False):
        result = {}
        for key, value in node.value:
            name = self.construct_object(key, deep=deep)
            if not isinstance(name, str) or name == "<<" or name in result:
                raise WorkflowError("workflow contains a duplicate or non-string mapping key")
            result[name] = self.construct_object(value, deep=deep)
        return result


def load_workflow(root: Path, path: str) -> dict:
    try:
        value = yaml.load((root / path).read_text(encoding="utf-8"), Loader=WorkflowLoader)
    except (OSError, UnicodeError, yaml.YAMLError) as error:
        raise WorkflowError(f"cannot read workflow {path}") from error
    if not isinstance(value, dict) or not isinstance(value.get("jobs"), dict) or not value["jobs"]:
        raise WorkflowError(f"{path}: missing workflow jobs")
    return value


def local_call(value: object) -> str:
    if not isinstance(value, str) or re.fullmatch(r"\./\.github/workflows/[A-Za-z0-9_-]+\.ya?ml", value) is None:
        raise WorkflowError(f"CI workflow call is not bound to its own commit: {value!r}")
    return value[2:]


def dependencies(job: dict) -> list[str]:
    value = job.get("needs", [])
    if isinstance(value, str):
        value = [value]
    if not isinstance(value, list) or any(not isinstance(item, str) for item in value) or len(set(value)) != len(value):
        raise WorkflowError("invalid or repeated native job dependency")
    return value


def dependency_graph(jobs: dict) -> dict[str, list[str]]:
    """Validate the native DAG once, including jobs outside a selected closure."""
    edges = {}
    for name, job in jobs.items():
        if not isinstance(job, dict):
            raise WorkflowError(f"{name}: invalid native job")
        edges[name] = dependencies(job)
        if not set(edges[name]) <= jobs.keys():
            raise WorkflowError(f"{name}: missing native dependency")
    visiting, visited = set(), set()

    def visit(name: str) -> None:
        if name in visiting:
            raise WorkflowError(f"{name}: cyclic native dependency")
        if name in visited:
            return
        visiting.add(name)
        for dependency in edges[name]:
            visit(dependency)
        visiting.remove(name)
        visited.add(name)

    for name in jobs:
        visit(name)
    return edges


def producer_result(job: dict, event: str) -> str:
    if job.get("continue-on-error", "false") != "false":
        raise WorkflowError("native CI producer is optional")
    guard = job.get("if")
    if guard in (None, "always()", "${{ always() }}"):
        return "success"
    if guard == QUALIFICATION:
        return "skipped" if event == "pull_request" else "success"
    raise WorkflowError("native CI producer has an unknown job guard")


def publication_graph(root: Path) -> tuple[dict, dict[str, list[str]]]:
    workflow = load_workflow(root, ".github/workflows/ci.yml")
    trigger = workflow.get("on")
    if isinstance(trigger, (str, list)):
        valid = trigger in ("push", ["push"])
    elif isinstance(trigger, dict) and set(trigger) == {"push"}:
        # Prove every main branch push is selected without implementing GitHub globs.
        valid = trigger["push"] in ("", {}, {"branches": ["main"]})
    else:
        valid = False
    if not valid:
        raise WorkflowError("publication CI does not select every main branch push")
    jobs = workflow["jobs"]
    return jobs, dependency_graph(jobs)


def verify_publication_qualification(root: Path, path: str, jobs: dict, edges: dict) -> None:
    matches = [name for name, job in jobs.items() if "uses" in job and local_call(job["uses"]) == path]
    if len(matches) != 1:
        raise WorkflowError(f"{path}: publication lacks its full native qualification")
    visited = set()

    def visit(name: str) -> None:
        if name in visited:
            return
        visited.add(name)
        job = jobs[name]
        if producer_result(job, "push") != "success":
            raise WorkflowError(f"{name}: publication dependency is skipped")
        if "uses" in job:
            shape(root, local_call(job["uses"]))
        elif not job.get("runs-on") or not job.get("steps"):
            raise WorkflowError(f"{name}: publication dependency has no executable job")
        for dependency in edges[name]:
            visit(dependency)

    visit(matches[0])


def shape(root: Path, path: str, ancestors: tuple[str, ...] = ()) -> tuple:
    if path in ancestors:
        raise WorkflowError("recursive CI workflow call")
    workflow = load_workflow(root, path)
    call = workflow.get("on", {}).get("workflow_call")
    if not isinstance(call, dict) or call.get("outputs", {}).get("checks", {}).get("value") != CHECKS:
        raise WorkflowError(f"{path}: native check results are not exported")
    jobs = workflow["jobs"]
    dependency_graph(jobs)
    children = []
    for name, job in jobs.items():
        if not isinstance(job, dict) or job.get("continue-on-error", "false") != "false":
            raise WorkflowError(f"{path}/{name}: optional check")
        if job.get("if", "always()") not in {"always()", "${{ always() }}"}:
            raise WorkflowError(f"{path}/{name}: check has a conditional job guard")
        if "uses" in job:
            children.append(shape(root, local_call(job["uses"]), (*ancestors, path)))
        elif not job.get("runs-on") or not job.get("steps"):
            raise WorkflowError(f"{path}/{name}: check has no executable job")
    return len(jobs), tuple(sorted(children))


def graph(root: Path, event: str = "merge_group") -> dict[str, Producer]:
    if event not in EVENTS:
        raise WorkflowError("unsupported required CI event")
    entry = load_workflow(root, ENTRY)
    if set(entry.get("on", {})) != {"pull_request", "merge_group"}:
        raise WorkflowError("required CI must run on both PR and merge queue")
    jobs = entry["jobs"]
    dependency_graph(jobs)
    gate = jobs.get("gate", {})
    if (gate.get("name") != "CI" or gate.get("if") != "${{ always() }}" or
            gate.get("continue-on-error", "false") != "false"):
        raise WorkflowError("required CI must run after unsuccessful dependencies")
    if set(dependencies(gate)) != jobs.keys() - {"gate"}:
        raise WorkflowError("required CI does not depend on every producer")
    final = gate.get("steps", [])[-1]
    if final.get("env", {}).get("CI_NEEDS") != "${{ toJSON(needs) }}" or final.get("env", {}).get("GATE_EVENT") != "${{ github.event_name }}":
        raise WorkflowError("required CI is not bound to its native dependencies and event")
    if (final.get("run") != COMPLETE_COMMAND or "if" in final or
            final.get("continue-on-error", "false") != "false"):
        raise WorkflowError("required CI does not execute its native result validation")
    for name, job in jobs.items():
        for step in job.get("steps", []):
            if str(step.get("uses", "")).startswith("actions/checkout@") and step.get("with", {}).get("ref") != "${{ github.workflow_sha }}":
                raise WorkflowError(f"{name}: CI source checkout differs from workflow snapshot")
    result = {}
    qualifications = []
    for name, job in jobs.items():
        if name == "gate":
            continue
        guard = job.get("if")
        status = producer_result(job, event)
        if guard == QUALIFICATION:
            if "uses" not in job:
                raise WorkflowError("event qualification must call a complete reusable graph")
            qualifications.append(local_call(job["uses"]))
        result[name] = Producer(status, shape(root, local_call(job["uses"])) if "uses" in job else None)
    if not any(value.checks is not None for value in result.values()):
        raise WorkflowError("required CI has no reusable checks")
    # Publication retains a full same-commit call for every deferred qualification.
    push_jobs, push_edges = publication_graph(root)
    for path in qualifications:
        verify_publication_qualification(root, path, push_jobs, push_edges)
    for path in sorted((root / ".github/workflows").glob("*.y*ml")):
        relative = path.relative_to(root).as_posix()
        triggers = load_workflow(root, relative).get("on", {})
        if isinstance(triggers, str):
            triggers = [triggers]
        if relative != ENTRY and any(event in triggers for event in ("pull_request", "pull_request_target", "merge_group")):
            raise WorkflowError(f"independent pre-merge workflow: {relative}")
    return result


def verify_checks(value: object, expected: tuple) -> None:
    if isinstance(value, str):
        value = json.loads(value)
    if not isinstance(value, dict) or set(value) != {"results", "children"}:
        raise WorkflowError("missing or unknown native check result tree")
    results, children = value["results"], value["children"]
    if not isinstance(results, list) or len(results) != expected[0] or not results or any(item != "success" for item in results):
        raise WorkflowError("native checks are missing, unsuccessful or unknown")
    if not isinstance(children, list) or len(children) != len(expected[1]):
        raise WorkflowError("nested native check results are missing or unexpected")
    # Object filters do not promise ordering. Every child must match one declared subtree.
    remaining = list(expected[1])
    for child in children:
        for index, child_shape in enumerate(remaining):
            try:
                verify_checks(child, child_shape)
            except (WorkflowError, ValueError, TypeError):
                continue
            remaining.pop(index)
            break
        else:
            raise WorkflowError("nested native check result does not match the workflow graph")


def verify_needs(value: object, expected: dict[str, Producer]) -> None:
    if not isinstance(value, dict) or not value or set(value) != set(expected):
        raise WorkflowError("native dependencies are missing or unexpected")
    for name, producer in expected.items():
        job = value[name]
        if not isinstance(job, dict) or job.get("result") != producer.result:
            raise WorkflowError(f"{name}: native dependency did not satisfy its event")
        outputs = job.get("outputs", {})
        if not isinstance(outputs, dict):
            raise WorkflowError(f"{name}: native dependency outputs are invalid")
        if producer.result == "skipped":
            if outputs not in ({}, {"checks": ""}):
                raise WorkflowError(f"{name}: skipped dependency exported check results")
        elif producer.checks is not None:
            verify_checks(outputs.get("checks"), producer.checks)


def main() -> None:
    if len(sys.argv) == 2 and sys.argv[1] == COMPLETE_MODE:
        expected = graph(ROOT, os.environ.get("GATE_EVENT"))
        verify_needs(json.loads(os.environ["CI_NEEDS"]), expected)
        print("Every declared CI producer satisfied its native workflow event.")
    elif len(sys.argv) == 1:
        graph(ROOT)
        print("All pre-merge checks belong to the required native CI graph.")
    else:
        raise WorkflowError("unsupported native CI operation")


if __name__ == "__main__":
    main()
