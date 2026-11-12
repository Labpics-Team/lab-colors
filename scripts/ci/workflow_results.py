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
CHECKS = "${{ format('{{\"results\":{0},\"children\":{1}}}', toJSON(jobs.*.result), toJSON(jobs.*.outputs.checks)) }}"
QUALIFICATION = "${{ always() && github.event_name != 'pull_request' }}"
EVENTS = {"pull_request", "merge_group"}


class WorkflowError(AssertionError):
    pass


@dataclass(frozen=True)
class NamedChecks:
    results: tuple[tuple[str, str], ...]
    children: tuple


def checks_export(jobs: dict) -> str:
    """Project only named statuses; report and distribution outputs stay local."""
    names = sorted(jobs)
    if not names or any(re.fullmatch(r"[A-Za-z_][A-Za-z0-9_-]*", name) is None for name in names):
        raise WorkflowError("invalid native job identifier")
    fields = ",".join(f'"{name}":{{{index}}}' for index, name in enumerate(names))
    template = '{{"results":{{' + fields + '}},"children":{' + str(len(names)) + '}}}'
    # Normalize each job object at the JSON boundary before projecting its result.
    values = [f"toJSON(fromJSON(toJSON(jobs.{name})).result)" for name in names]
    return "${{ format('" + template + "', " + ", ".join((*values, "toJSON(jobs.*.outputs.checks)")) + ") }}"


def project_checks(root: Path, path: str) -> None:
    """Regenerate the checked projection without rewriting the workflow."""
    path = local_call("./" + path)
    workflow = load_workflow(root, path)
    old = workflow.get("on", {}).get("workflow_call", {}).get("outputs", {}).get("checks", {}).get("value")
    source_path = root / path
    source = source_path.read_bytes().decode("utf-8")
    newline = "\r\n" if "\r\n" in source else "\n"
    source = source.replace("\r\n", "\n")
    block = re.compile(r"(\n      checks:\n(?:        (?!value:)[^\n]*\n)*)(        value:[^\n]*\n(?:          [^\n]*\n)*)")
    if not isinstance(old, str) or len(block.findall(source)) != 1:
        raise WorkflowError("native checks projection must have one literal value")
    expression = checks_export(workflow['jobs'])
    template, values = expression.split("', ", 1)
    lines = [template + "',", *values.split(", ")]
    for index in range(1, len(lines) - 1):
        lines[index] += ","
    replacement = "        value: >-\n" + "".join(f"          {line}\n" for line in lines)
    updated = block.sub(lambda match: match[1] + replacement, source)
    source_path.write_bytes(updated.replace("\n", newline).encode("utf-8"))


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


def shape(root: Path, path: str, event: str, ancestors: tuple[str, ...] = ()) -> tuple | NamedChecks:
    if path in ancestors:
        raise WorkflowError("recursive CI workflow call")
    workflow = load_workflow(root, path)
    call = workflow.get("on", {}).get("workflow_call")
    exported = call.get("outputs", {}).get("checks", {}).get("value") if isinstance(call, dict) else None
    jobs = workflow["jobs"]
    named = exported == checks_export(jobs)
    if not named and exported != CHECKS:
        raise WorkflowError(f"{path}: native check results are not exported")
    children = []
    results = []
    for name, job in jobs.items():
        if not isinstance(job, dict) or job.get("continue-on-error", "false") != "false":
            raise WorkflowError(f"{path}/{name}: optional check")
        guard = job.get("if", "always()")
        if guard == QUALIFICATION and named:
            status = "skipped" if event == "pull_request" else "success"
        elif guard in {"always()", "${{ always() }}"}:
            status = "success"
        else:
            raise WorkflowError(f"{path}/{name}: check has a conditional job guard")
        results.append((name, status))
        if not set(dependencies(job)) <= jobs.keys():
            raise WorkflowError(f"{path}/{name}: missing native dependency")
        if "uses" in job:
            if guard == QUALIFICATION:
                raise WorkflowError("event-scoped checks must not hide nested reusable results")
            children.append(shape(root, local_call(job["uses"]), event, (*ancestors, path)))
        elif not job.get("runs-on") or not job.get("steps"):
            raise WorkflowError(f"{path}/{name}: check has no executable job")
    children = tuple(sorted(children, key=repr))
    return NamedChecks(tuple(sorted(results)), children) if named else (len(jobs), children)


def graph(root: Path, event: str = "merge_group") -> dict[str, tuple | NamedChecks | None]:
    if event not in EVENTS:
        raise WorkflowError("unsupported required CI event")
    entry = load_workflow(root, ENTRY)
    if set(entry.get("on", {})) != {"pull_request", "merge_group"}:
        raise WorkflowError("required CI must run on both PR and merge queue")
    jobs = entry["jobs"]
    gate = jobs.get("gate", {})
    if gate.get("name") != "CI" or gate.get("if") != "${{ always() }}":
        raise WorkflowError("required CI must run after unsuccessful dependencies")
    if set(dependencies(gate)) != jobs.keys() - {"gate"}:
        raise WorkflowError("required CI does not depend on every producer")
    final = gate.get("steps", [])[-1]
    if final.get("env", {}).get("CI_NEEDS") != "${{ toJSON(needs) }}" or final.get("env", {}).get("GATE_EVENT") != "${{ github.event_name }}":
        raise WorkflowError("required CI is not bound to its native dependencies and event")
    for name, job in jobs.items():
        for step in job.get("steps", []):
            if str(step.get("uses", "")).startswith("actions/checkout@") and step.get("with", {}).get("ref") != "${{ github.workflow_sha }}":
                raise WorkflowError(f"{name}: CI source checkout differs from workflow snapshot")
    result = {}
    for name, job in jobs.items():
        if name == "gate":
            continue
        if "if" in job or job.get("continue-on-error", "false") != "false":
            raise WorkflowError(f"{name}: required CI producer is optional")
        result[name] = shape(root, local_call(job["uses"]), event) if "uses" in job else None
    if not any(value is not None for value in result.values()):
        raise WorkflowError("required CI has no reusable checks")
    for path in sorted((root / ".github/workflows").glob("*.y*ml")):
        relative = path.relative_to(root).as_posix()
        triggers = load_workflow(root, relative).get("on", {})
        if isinstance(triggers, str):
            triggers = [triggers]
        if relative != ENTRY and any(event in triggers for event in ("pull_request", "pull_request_target", "merge_group")):
            raise WorkflowError(f"independent pre-merge workflow: {relative}")
    return result


def verify_checks(value: object, expected: tuple | NamedChecks) -> None:
    if isinstance(value, str):
        value = json.loads(value)
    if not isinstance(value, dict) or set(value) != {"results", "children"}:
        raise WorkflowError("missing or unknown native check result tree")
    results, children = value["results"], value["children"]
    if isinstance(expected, NamedChecks):
        if not isinstance(results, dict) or results != dict(expected.results):
            raise WorkflowError("named native checks are missing, unsuccessful or outside their event")
        child_shapes = expected.children
    else:
        if not isinstance(results, list) or len(results) != expected[0] or not results or any(item != "success" for item in results):
            raise WorkflowError("native checks are missing, unsuccessful or unknown")
        child_shapes = expected[1]
    if not isinstance(children, list) or len(children) != len(child_shapes):
        raise WorkflowError("nested native check results are missing or unexpected")
    # Object filters do not promise ordering. Every child must match one declared subtree.
    remaining = list(child_shapes)
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


def verify_needs(value: object, expected: dict[str, tuple | NamedChecks | None]) -> None:
    if not isinstance(value, dict) or not value or set(value) != set(expected):
        raise WorkflowError("native dependencies are missing or unexpected")
    for name, child in expected.items():
        job = value[name]
        if not isinstance(job, dict) or job.get("result") != "success":
            raise WorkflowError(f"{name}: native dependency is not successful")
        if child is not None:
            verify_checks(job.get("outputs", {}).get("checks"), child)


def main() -> None:
    if len(sys.argv) == 3 and sys.argv[1] == "project":
        project_checks(ROOT, sys.argv[2])
        return
    if len(sys.argv) == 2 and sys.argv[1] == "complete":
        expected = graph(ROOT, os.environ.get("GATE_EVENT"))
        verify_needs(json.loads(os.environ["CI_NEEDS"]), expected)
        print("Every declared CI check satisfied its native workflow event.")
    else:
        graph(ROOT)
        print("All pre-merge checks belong to the required native CI graph.")


if __name__ == "__main__":
    main()
