"""PR and merge-queue evidence keeps the exact source and paired baseline."""

from pathlib import Path
import re
import unittest

from test_ci_storage import job


ROOT = Path(__file__).resolve().parents[1]
SOURCE = "github.event.pull_request.head.sha || github.event.merge_group.head_sha || github.sha"
HEAD = "github.event.pull_request.head.sha || github.event.merge_group.head_sha"
BASE = "github.event.pull_request.base.sha || github.event.merge_group.base_sha"
PAIRED = "github.event_name == 'pull_request' || github.event_name == 'merge_group'"
BENCHMARK = "github.event_name == 'pull_request' && contains(github.event.pull_request.labels.*.name, 'performance')"


def steps(source):
    return re.findall(r"(?ms)^      - .*?(?=^      - |\Z)", source)


def containing(values, marker):
    selected = [value for value in values if marker in value]
    if len(selected) != 1:
        raise AssertionError(f"expected one step containing {marker!r}")
    return selected[0]


def expression(source, key, expected):
    values = re.findall(r"(?m)^\s+" + re.escape(key) + r": ([^\r\n]+)$", source)
    normalized = [re.sub(r"\s+", " ", value.removeprefix("${{").removesuffix("}}").strip())
                  for value in values]
    if normalized != [expected]:
        raise AssertionError(f"{key} must bind {expected!r}, got {normalized!r}")


def validate_distribution(source):
    for kind in ("transport", "evaluate"):
        worker = job(source, kind + "-distribution")
        if re.search(r"(?m)^    (?:if|continue-on-error):", worker):
            raise AssertionError("distribution evidence must not be optional")
        entries = steps(worker)
        checkout = containing(entries, "uses: actions/checkout@")
        expression(checkout, "ref", SOURCE)
        control = containing(entries, "id: base\n")
        expression(control, "if", PAIRED)
        expression(control, "HEAD_SHA", HEAD)
        expression(control, "BASE_SHA", BASE)
        if "continue-on-error:" in control:
            raise AssertionError("paired baseline failure must fail the job")
        for command in ('git merge-base "$HEAD_SHA" "$BASE_SHA"',
                        'git worktree add --detach "$BASE_ROOT" "$CONTROL_SHA"',
                        'echo "sha=$CONTROL_SHA" >> "$GITHUB_OUTPUT"'):
            if command not in control:
                raise AssertionError("paired baseline lost its Git binding: " + command)
        generate = containing(entries, f'node scripts/{kind}-distribution.mjs "${{args[@]}}"')
        expression(generate, "SOURCE_SHA", SOURCE)
        expression(generate, "BASE_SHA", "steps.base.outputs.sha || ''")
        if 'args+=(--baseline-binary "$BASE_BINARY" --baseline-sha "$BASE_SHA")' not in generate:
            raise AssertionError("distribution verifier lost its paired baseline inputs")
        handoff = containing(entries, "id: handoff\n")
        expression(handoff, "SOURCE_SHA", SOURCE)


def validate_package(ci, package):
    expression(job(ci, "wasm"), "program-benchmark", BENCHMARK)
    build = containing(steps(job(package, "wasm")), "PROGRAM_BENCHMARK:")
    expression(build, "PROGRAM_BASE_SHA", BASE)
    expression(build, "PROGRAM_HEAD_SHA", SOURCE)
    if re.search(r"(?m)^(?:      - |        )if:", build) or "continue-on-error:" in build:
        raise AssertionError("ordinary package verification must stay unconditional")


def validate_gate(source):
    final = containing(steps(job(source, "gate")), "Require every result")
    expression(final, "GATE_EVENT", "github.event_name")
    expression(final, "CI_NEEDS", "toJSON(needs)")


class MergeGroupEvidenceTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.ci = (ROOT / ".github/workflows/ci-worker.yml").read_text(encoding="utf-8")
        cls.package = (ROOT / ".github/workflows/package-worker.yml").read_text(encoding="utf-8")
        cls.gate = (ROOT / ".github/workflows/ci-gate.yml").read_text(encoding="utf-8")

    def test_distribution_pairs_pr_and_queue_sources_with_their_base(self):
        validate_distribution(self.ci)

    def test_optional_pr_benchmark_does_not_replace_package_checks(self):
        validate_package(self.ci, self.package)

    def test_package_verification_cannot_be_conditional_in_any_step_key_order(self):
        build = containing(steps(job(self.package, "wasm")), "PROGRAM_BENCHMARK:")
        first = build.splitlines()[0]
        self.assertTrue(first.startswith("      - name:"))
        for optional in ("if: false", "continue-on-error: true"):
            for replacement in (
                first.replace("- name:", f"- {optional}\n        name:"),
                first + f"\n        {optional}",
            ):
                with self.subTest(optional=optional, first_key=replacement.splitlines()[0]):
                    changed = build.replace(first, replacement, 1)
                    with self.assertRaises(AssertionError):
                        validate_package(self.ci, self.package.replace(build, changed, 1))

    def test_required_ci_uses_the_native_event_and_dependencies(self):
        validate_gate(self.gate)

    def test_lost_queue_identity_and_baseline_are_rejected(self):
        for before, after in (
            (" || github.event.merge_group.head_sha", ""),
            (" || github.event.merge_group.base_sha", ""),
            (" || github.event_name == 'merge_group'", ""),
            ('git merge-base "$HEAD_SHA" "$BASE_SHA"', 'git rev-parse "$HEAD_SHA"'),
            ('args+=(--baseline-binary "$BASE_BINARY" --baseline-sha "$BASE_SHA")', ":"),
            ("        id: base\n", "        id: base\n        continue-on-error: true\n"),
        ):
            with self.subTest(before=before):
                self.assertTrue(before in self.ci, f"mutation target absent: {before}")
                with self.assertRaises(AssertionError):
                    validate_distribution(self.ci.replace(before, after))

    def test_package_event_inputs_and_opt_in_cannot_silently_change(self):
        for before, after in (
            (" || github.event.merge_group.head_sha", ""),
            (" || github.event.merge_group.base_sha", ""),
        ):
            with self.subTest(before=before):
                self.assertTrue(before in self.package, f"mutation target absent: {before}")
                with self.assertRaises(AssertionError):
                    validate_package(self.ci, self.package.replace(before, after))
        for replacement in ("true", "false", "github.event_name == 'merge_group'"):
            with self.subTest(benchmark=replacement):
                self.assertTrue(BENCHMARK in self.ci, "benchmark opt-in is absent")
                with self.assertRaises(AssertionError):
                    validate_package(self.ci.replace(BENCHMARK, replacement), self.package)


if __name__ == "__main__":
    unittest.main()
