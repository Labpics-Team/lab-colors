"""Доверенный источник и фактически выполняемая процедура CI до загрузки кода."""

from __future__ import annotations

import inspect
import json
import os
import re


def validate_owned_source(environment):
    """Служебные поля GitHub определяют источник; вход вызывающего файла не выдаёт полномочия."""
    import json, re

    def required(name):
        value = environment.get(name, "")
        if not isinstance(value, str) or not value:
            raise ValueError("INCOMPLETE: missing native " + name)
        return value

    def sha(name):
        value = required(name)
        if re.fullmatch(r"[0-9a-f]{40}", value) is None or value == "0" * 40:
            raise ValueError("INCOMPLETE: invalid native " + name)
        return value

    if environment.get("GITHUB_ACTIONS") != "true":
        raise ValueError("INCOMPLETE: native GitHub Actions context is required")
    repository = required("CI_REPOSITORY")
    repository_id = required("CI_REPOSITORY_ID")
    if repository != "Labpics-Team/lab-colors" or repository_id != "1221255339":
        raise ValueError("REFUSED: source repository is outside the owned scope")

    try:
        native_job = json.loads(required("CI_NATIVE_JOB"))
    except (ValueError, TypeError) as error:
        raise ValueError("INCOMPLETE: invalid native job context") from error
    if not isinstance(native_job, dict):
        raise ValueError("INCOMPLETE: native job context is not an object")
    environment = dict(environment)
    for name, property_name in (
        ("CI_RECIPE_SHA", "workflow_sha"),
        ("CI_RECIPE_REF", "workflow_ref"),
        ("CI_RECIPE_REPOSITORY", "workflow_repository"),
        ("CI_RECIPE_PATH", "workflow_file_path"),
        ("CI_CHECK_RUN_ID", "check_run_id"),
    ):
        value = native_job.get(property_name)
        if property_name == "check_run_id" and type(value) is int and value > 0:
            value = str(value)
        if not isinstance(value, str) or not value:
            raise ValueError("INCOMPLETE: missing native job." + property_name)
        environment[name] = value
    source_sha = sha("CI_SOURCE_SHA")
    caller_sha = sha("CI_CALLER_SHA")
    recipe_sha = sha("CI_RECIPE_SHA")
    recipe_repository = required("CI_RECIPE_REPOSITORY")
    recipe_path = required("CI_RECIPE_PATH")
    expected_path = required("CI_EXPECTED_RECIPE_PATH")
    recipe_ref = required("CI_RECIPE_REF")
    if recipe_repository != repository or recipe_path != expected_path:
        raise ValueError("REFUSED: executing recipe repository or path differs")
    if re.fullmatch(r"\.github/workflows/[a-z0-9-]+\.yml", recipe_path) is None:
        raise ValueError("REFUSED: invalid executing recipe path")
    prefix = repository + "/" + recipe_path + "@"
    if not recipe_ref.startswith(prefix) or not recipe_ref[len(prefix):]:
        raise ValueError("INCOMPLETE: executing recipe ref is not bound to its file")

    protected_main = recipe_ref == prefix + "refs/heads/main"

    event = required("CI_EVENT")
    ref = required("CI_REF")
    if event == "pull_request":
        head_repository = required("CI_HEAD_REPOSITORY")
        head_id = required("CI_HEAD_REPOSITORY_ID")
        sha("CI_HEAD_SHA")
        if head_repository != repository or head_id != repository_id:
            raise ValueError("REFUSED: foreign pull request head")
        if re.fullmatch(r"refs/pull/[1-9][0-9]*/merge", ref) is None:
            raise ValueError("REFUSED: unsupported pull request source ref")
    elif event == "merge_group":
        if not ref.startswith("refs/heads/gh-readonly-queue/main/"):
            raise ValueError("REFUSED: merge group is outside main")
    elif event == "push":
        if ref != "refs/heads/main" and not ref.startswith("refs/tags/colors-v"):
            raise ValueError("REFUSED: push is outside main or the release scope")
    elif event in ("workflow_dispatch", "schedule"):
        if protected_main and ref != "refs/heads/main":
            raise ValueError("REFUSED: manual or scheduled source is outside main")
        if not protected_main and not ref.startswith("refs/heads/"):
            raise ValueError("REFUSED: bootstrap source is not an owned branch")
    else:
        raise ValueError("REFUSED: unsupported source event")

    runner_environment = required("CI_RUNNER_ENVIRONMENT")
    runner_os = required("CI_RUNNER_OS")
    runner_arch = required("CI_RUNNER_ARCH")
    runner_name = required("CI_RUNNER_NAME")
    expected_environment = required("CI_REQUIRED_RUNNER_ENVIRONMENT")
    if runner_environment not in ("self-hosted", "github-hosted"):
        raise ValueError("INCOMPLETE: unknown runner environment")
    if expected_environment not in ("any", "self-hosted", "github-hosted"):
        raise ValueError("REFUSED: unknown execution requirement")
    if expected_environment != "any" and runner_environment != expected_environment:
        raise ValueError("REFUSED: required runner environment is unavailable")
    for name in ("CI_RUN_ID", "CI_RUN_ATTEMPT", "CI_CHECK_RUN_ID"):
        if re.fullmatch(r"[1-9][0-9]*", required(name)) is None:
            raise ValueError("INCOMPLETE: invalid native " + name)
    if protected_main:
        if event in ("push", "workflow_dispatch", "schedule") and recipe_sha != source_sha:
            raise ValueError("REFUSED: exact main or release recipe has not executed")
    elif not source_sha == caller_sha == recipe_sha:
        # Допуск конкретного FULLSHA принадлежит native Listener, а не этому
        # guard. Здесь разрешённый snapshot не вправе подменять свою identity.
        raise ValueError("REFUSED: admitted snapshot differs from source or caller")
    binding = {
        "source_sha": source_sha,
        "head_sha": environment.get("CI_HEAD_SHA", "") if event == "pull_request" else "",
        "caller_sha": caller_sha,
        "recipe_sha": recipe_sha,
        "recipe_ref": recipe_ref,
        "recipe_repository": recipe_repository,
        "recipe_path": recipe_path,
        "runner_environment": runner_environment,
        "runner_os": runner_os,
        "runner_arch": runner_arch,
        "runner_name": runner_name,
        "run_id": environment["CI_RUN_ID"],
        "run_attempt": environment["CI_RUN_ATTEMPT"],
        "check_run_id": environment["CI_CHECK_RUN_ID"],
        "identity_relation": "same-revision" if source_sha == recipe_sha else "distinct-revisions",
    }
    for key, value in binding.items():
        if re.search(r"[\x00-\x1f\x7f]", value) is not None:
            raise ValueError("REFUSED: control character in native " + key)
    return binding


def guard_program():
    """Одна реализация проецируется в доверенный YAML; action@main не перезагружается."""
    return (
        "import json, os, sys\n"
        + inspect.getsource(validate_owned_source)
        + "\n"
        + "try:\n"
        + "    binding = validate_owned_source(os.environ)\n"
        + "except ValueError as error:\n"
        + "    raise SystemExit(str(error))\n"
        + "with open(os.environ['GITHUB_ENV'], 'a', encoding='utf-8') as output:\n"
        + "    for key, value in binding.items():\n"
        + "        output.write('CI_BOUND_' + key.upper() + '=' + value + '\\n')\n"
        + "    output.write('CI_VERIFIED_SOURCE_SHA=' + binding['source_sha'] + '\\n')\n"
        + "    output.write('CI_VERIFIED_CALLER_SHA=' + binding['caller_sha'] + '\\n')\n"
        + "    output.write('CI_VERIFIED_RECIPE_SHA=' + binding['recipe_sha'] + '\\n')\n"
        + "with open(os.environ['GITHUB_OUTPUT'], 'a', encoding='utf-8') as output:\n"
        + "    for key, value in binding.items():\n"
        + "        output.write(key + '=' + value + '\\n')\n"
        + "print(json.dumps(binding, sort_keys=True))\n"
        + "with open(os.environ['GITHUB_STEP_SUMMARY'], 'a', encoding='utf-8') as output:\n"
        + "    output.write('Source ' + binding['source_sha'] + '\\n\\nCaller ' + binding['caller_sha'] + '\\n\\nExecuting recipe ' + binding['recipe_sha'] + '\\n\\nIdentity relation ' + binding['identity_relation'] + '\\n')\n"
    )
