"""Локальные байты пакета и изоляция полномочий публикации."""
from pathlib import Path
import re
import unittest

ROOT=Path(__file__).resolve().parents[1]


def job(source, name):
    match=re.search(r"(?ms)^  "+re.escape(name)+r":\n.*?(?=^  [a-zA-Z][a-zA-Z0-9_-]*:|\Z)",source)
    if not match:
        raise AssertionError("job absent: "+name)
    return match.group()


def validate(ci, package, release):
    for caller in [job(ci,"wasm"),job(release,"prepare")]:
        if "    uses: ./.github/workflows/package-worker.yml\n" not in caller:
            raise AssertionError("package preparation owner changed")
        if "secrets:" in caller or "environment:" in caller:
            raise AssertionError("preparation acquired publisher permissions")
    if "environment:" in package or "secrets." in package:
        raise AssertionError("package checks acquired credentials")
    owner=job(package,"wasm")
    for command in ["build_runtime", "cargo clean", "diff --no-dereference --recursive",
                    "npm run typecheck", "npm test", "npm run release:verify",
                    "wasm-pack test --headless --chrome", "node scripts/test-program-runtime-browser.mjs",
                    'test "$(node -p process.versions.node)" = "$NODE_CONSUMER_FLOOR"',
                    'node scripts/verify-package-release.mjs --package-smoke "$VERIFIED_TARBALL"']:
        blocks=re.findall(r"(?ms)^      - .*?(?=^      - |\Z)",owner)
        matches=[block for block in blocks if command in block]
        if len(matches)!=1 or "\n        if:" in matches[0] or "continue-on-error:" in matches[0]:
            raise AssertionError("package verification missing or conditional: "+command)
    if "uses: actions/download-artifact@" in owner:
        raise AssertionError("local consumers depend on external storage")
    uploads=re.findall(r"(?ms)^      - name: upload verified npm tarball and manifest\n(.*?)(?=^      - name:|\Z)",owner)
    if len(uploads)!=1 or "retention-days: 1\n" not in uploads[0]:
        raise AssertionError("release handoff has no bounded lifetime")
    expected_guard = "github.event_name == 'push' && github.ref_type == 'tag' && startsWith(github.ref_name, 'colors-v')"
    upload_conditions = re.findall(r"(?m)^        if: ([^\r\n]+)$", uploads[0])
    if upload_conditions != [expected_guard] or re.search(r"(?m)^        continue-on-error:", uploads[0]):
        raise AssertionError("release handoff became optional or lost its exact release guard")
    if owner.count("uses: actions/upload-artifact@") != 1:
        raise AssertionError("package diagnostics must not use remote artifact storage")
    publisher=job(release,"publish")
    if "    needs: prepare\n" not in publisher or "    environment: npm-publish\n" not in publisher:
        raise AssertionError("publish bypasses isolated verification")
    if "run-id:" in publisher or "steps.canonical-runs.outputs.ci_run_attempt" in publisher:
        raise AssertionError("release requires a retained ordinary CI archive")


class CiStorageTests(unittest.TestCase):
    def setUp(self):
        self.sources=[(ROOT/".github/workflows"/name).read_text() for name in
                      ["ci-worker.yml","package-worker.yml","publish-worker.yml"]]

    def test_current_package_graph_retains_only_the_release_handoff(self):
        validate(*self.sources)

    def test_skipped_floor_external_input_and_changed_permissions_are_rejected(self):
        for index,before,after in [
            (0,'    uses: ./.github/workflows/package-worker.yml','    uses: ./.github/workflows/other.yml'),
            (1,'npm run release:verify','npm --version'),
            (1,'npm run typecheck','echo typecheck'),
            (1,'node scripts/test-program-runtime-browser.mjs','echo browser'),
            (1,'--package-smoke "$VERIFIED_TARBALL"','--help'),
            (1,"github.ref_type == 'tag'","true"),
            (1,"if: github.event_name == 'push'", "if: false && github.event_name == 'push'"),
            (1,"retention-days: 1", "retention-days: 1\n        continue-on-error: true"),
            (1,'retention-days: 1','retention-days: 90'),
            (2,'    needs: prepare','    needs: ignored'),
            (2,'    environment: npm-publish','    environment: other'),
        ]:
            with self.subTest(before=before):
                values=list(self.sources)
                self.assertIn(before,values[index])
                values[index]=values[index].replace(before,after)
                with self.assertRaises(AssertionError):
                    validate(*values)
        values=list(self.sources)
        values[1]+='\n      - uses: actions/download-artifact@0000000000000000000000000000000000000000\n'
        with self.assertRaises(AssertionError):
            validate(*values)


def job_level_optional(source, name):
    return re.search(r"(?m)^    (?:if|continue-on-error):", job(source, name)) is not None


def validate_native_storage(source):
    if source.count("uses: actions/upload-artifact@") != 1:
        raise AssertionError("native CI may only transfer the Android SDK")
    build = job(source, "android-build")
    if job_level_optional(source, "android-build"):
        raise AssertionError("Android SDK build job became optional")
    blocks = re.findall(r"(?ms)^      - .*?(?=^      - |\Z)", build)
    upload = [block for block in blocks if "uses: actions/upload-artifact@" in block]
    if len(upload) != 1:
        raise AssertionError("Android SDK transfer missing")
    for required in ("name: android-sdk-", "if-no-files-found: error"):
        if required not in upload[0]:
            raise AssertionError("Android SDK transfer changed: " + required)
    if not re.search(r"(?m)^[ \t]*retention-days:[ \t]*1[ \t]*$", upload[0]):
        raise AssertionError("Android SDK transfer must expire after one day")
    if "continue-on-error:" in upload[0] or "\n        if:" in upload[0]:
        raise AssertionError("Android SDK transfer became optional")
    consumer = job(source, "android-consumer")
    if job_level_optional(source, "android-consumer"):
        raise AssertionError("external Android consumer job became optional")
    consumer_steps = re.findall(r"(?ms)^      - .*?(?=^      - |\Z)", consumer)
    for required_step in (
        "uses: actions/download-artifact@", "run: python3 bindings/kotlin/ci/prepare-consumer.py",
        'run: python3 bindings/kotlin/ci/test-consumer.py "$RUNNER_TEMP/android-consumer" --assemble',
        "uses: reactivecircus/android-emulator-runner@",
    ):
        matching = [block for block in consumer_steps if required_step in block]
        if len(matching) != 1 or re.search(r"(?m)^(?:        |      - )(?:if|continue-on-error):", matching[0]):
            raise AssertionError("external Android consumer step missing or optional: " + required_step)
    # The separate inventory command is deliberately conditional on failure; it
    # must not be mistaken for the mandatory external compile step above.
    emulator = next(block for block in consumer_steps
                    if "uses: reactivecircus/android-emulator-runner@" in block)
    expected_runtime = ('script: python3 bindings/kotlin/ci/test-consumer.py '
                        '"$RUNNER_TEMP/android-consumer" ' + '$' + '{{ matrix.page }}')
    runtime_commands = [line.strip() for line in emulator.splitlines()
                        if line.lstrip().startswith("script:")]
    if runtime_commands != [expected_runtime]:
        raise AssertionError("external Android runtime consumer is not executed in the emulator")
    for required in ("needs: android-build", "uses: actions/download-artifact@",
                     "name: android-sdk-", "prepare-consumer.py", "test-consumer.py"):
        if required not in consumer:
            raise AssertionError("external Android consumer missing: " + required)
    if job_level_optional(source, "apple-package"):
        raise AssertionError("external Apple verification job became optional")
    apple_steps = re.findall(r"(?ms)^      - .*?(?=^      - |\Z)", job(source, "apple-package"))
    for required in ("build-apple.py", "test-apple.py"):
        matching = [block for block in apple_steps if "run: python3 bindings/swift/ci/" + required in block]
        if len(matching) != 1:
            raise AssertionError("external Apple verification missing: " + required)
        if re.search(r"(?m)^(?:        |      - )(?:if|continue-on-error):", matching[0]):
            raise AssertionError("external Apple verification became optional: " + required)


def validate_mutation_storage(source):
    # Shards and their manifest cross job boundaries; the aggregate report
    # already appears in job logs and must never consume artifact quota.
    if source.count("uses: actions/upload-artifact@") != 2:
        raise AssertionError("mutation CI may only transfer its manifest and shards")
    manifest = job(source, "manifest")
    shard = job(source, "shard")
    aggregate = job(source, "aggregate")
    if ("uses: actions/upload-artifact@" not in manifest
            or "name: mutation-manifest" not in manifest):
        raise AssertionError("mutation manifest handoff missing")
    if ("uses: actions/upload-artifact@" not in shard
            or "name: mutation-shard-" not in shard):
        raise AssertionError("mutation shard handoff missing")
    # Shards are only a live workflow handoff. They must not retain 32
    # archives for a month after aggregation finishes.
    for owner, transfer in ((manifest, "mutation-manifest"), (shard, "mutation-shard-")):
        blocks = re.findall(r"(?ms)^      - .*?(?=^      - |\Z)", owner)
        uploads = [block for block in blocks if "uses: actions/upload-artifact@" in block]
        if (len(uploads) != 1 or "name: " + transfer not in uploads[0]
                or not re.search(r"(?m)^          retention-days: 2[ \t]*$", uploads[0])):
            raise AssertionError("mutation transfer must expire after two days: " + transfer)
    if "uses: actions/upload-artifact@" in aggregate or "name: mutation-report" in aggregate:
        raise AssertionError("aggregate diagnostic upload returned")
    if (not re.search(r"(?m)^    if: always\(\)$", aggregate)
            or re.search(r"(?m)^    continue-on-error:", aggregate)):
        raise AssertionError("mutation aggregation job must run after failures")
    steps = re.findall(r"(?ms)^      - .*?(?=^      - |\Z)", aggregate)
    aggregation_steps = [step for step in steps
                         if step.startswith("      - name: verify exact non-overlapping aggregate\n")]
    if len(aggregation_steps) != 1 or re.search(
            r"(?m)^        (?:if|continue-on-error):", aggregation_steps[0]):
        raise AssertionError("mutation aggregation step is missing or optional")
    # These are real shell pipelines: pipefail keeps the exit status, and no
    # command may mask it after either materialization or aggregation.
    commands = [line.strip() for line in aggregation_steps[0].splitlines()]
    tee_command = '2>&1 | tee -a "$diagnostic"'
    if (commands.count(tee_command) != 2 or commands[-1] != tee_command
            or "          set -euo pipefail\n" not in aggregation_steps[0]):
        raise AssertionError("mutation aggregation can conceal an execution failure")
    # The aggregator executes the script from the materialized source tree.
    # Match the actual shell command, not a filename fragment that never
    # appears when the path is prefixed by "$source_root".
    if not re.search(
        r'(?m)^[ \t]+python3 "\$source_root/scripts/mutation\.py" aggregate[ \t]*\\[ \t]*$',
        aggregation_steps[0],
    ):
        raise AssertionError("mutation aggregation cannot be skipped")


class NativeStorageTests(unittest.TestCase):
    def setUp(self):
        self.source = (ROOT / ".github/workflows/native-conformance-worker.yml").read_text()
        self.mutation_source = (ROOT / ".github/workflows/mutation-worker.yml").read_text()

    def test_only_sdk_transfer_survives(self):
        validate_native_storage(self.source)
        validate_mutation_storage(self.mutation_source)

    def test_reintroduced_diagnostics_and_weakened_transfer_are_rejected(self):
        for before, after in [
            ("retention-days: 1", "retention-days: 7"),
            ("retention-days: 1", "retention-days: 14"),
            ("if-no-files-found: error", "if-no-files-found: ignore"),
            ("needs: android-build", "needs: other"),
            ("test-apple.py", "skip-apple.py"),
            ("script: python3 bindings/kotlin/ci/test-consumer.py", "script: echo skip-runtime"),
            ('run: python3 bindings/kotlin/ci/test-consumer.py "$RUNNER_TEMP/android-consumer" --assemble',
             'run: echo skip-external-assembly'),
        ]:
            with self.subTest(before=before):
                self.assertIn(before, self.source)
                with self.assertRaises(AssertionError):
                    validate_native_storage(self.source.replace(before, after))
        runtime = ('script: python3 bindings/kotlin/ci/test-consumer.py '
                   '"$RUNNER_TEMP/android-consumer" ' + '$' + '{{ matrix.page }}')
        self.assertIn(runtime, self.source)
        with self.assertRaises(AssertionError):
            validate_native_storage(self.source.replace(runtime, runtime + " || true", 1))
        for command in ("build-apple.py", "test-apple.py"):
            for setting in ("if: false", "continue-on-error: true"):
                with self.subTest(command=command, setting=setting):
                    original = "        run: python3 bindings/swift/ci/" + command
                    changed = "        " + setting + "\n" + original
                    self.assertIn(original, self.source)
                    with self.assertRaises(AssertionError):
                        validate_native_storage(self.source.replace(original, changed))
        for name in ("android-build", "android-consumer", "apple-package"):
            for setting in ("if: false", "continue-on-error: true"):
                with self.subTest(job=name, setting=setting):
                    original = "  " + name + ":\n"
                    self.assertIn(original, self.source)
                    changed = self.source.replace(original, original + "    " + setting + "\n", 1)
                    with self.assertRaises(AssertionError):
                        validate_native_storage(changed)
        for step in (
            "      - uses: actions/download-artifact@",
            "        run: python3 bindings/kotlin/ci/prepare-consumer.py",
            "        run: python3 bindings/kotlin/ci/test-consumer.py",
            "      - uses: reactivecircus/android-emulator-runner@",
        ):
            with self.subTest(step=step):
                self.assertIn(step, self.source)
                if step.startswith("      -"):
                    changed = self.source.replace(step, "      - if: false\n        " + step.strip().removeprefix("- "), 1)
                else:
                    changed = self.source.replace(step, "        if: false\n" + step, 1)
                with self.assertRaises(AssertionError):
                    validate_native_storage(changed)
        with self.assertRaises(AssertionError):
            validate_native_storage(self.source + "\n      - uses: actions/upload-artifact@v4\n")
        with self.assertRaises(AssertionError):
            validate_mutation_storage(self.mutation_source.replace(
                "      - name: verify exact non-overlapping aggregate",
                "      - uses: actions/upload-artifact@v4\n"
                "      - name: verify exact non-overlapping aggregate", 1))
        with self.assertRaises(AssertionError):
            validate_mutation_storage(self.mutation_source.replace(
                'python3 "$source_root/scripts/mutation.py" aggregate',
                'echo skipped-aggregate', 1))
        aggregate_job = "  aggregate:\n    name: prove complete mutation population\n    if: always()\n"
        self.assertIn(aggregate_job, self.mutation_source)
        for replacement in (
                aggregate_job.replace("    if: always()\n", "    if: false\n"),
                aggregate_job + "    continue-on-error: true\n"):
            with self.subTest(aggregate_job=replacement):
                with self.assertRaises(AssertionError):
                    validate_mutation_storage(self.mutation_source.replace(aggregate_job, replacement, 1))
        tee_line = '          2>&1 | tee -a "$diagnostic"\n'
        self.assertEqual(self.mutation_source.count(tee_line), 2)
        prefix, separator, suffix = self.mutation_source.rpartition(tee_line)
        with self.assertRaises(AssertionError):
            validate_mutation_storage(prefix + separator.rstrip("\n") + " || true\n" + suffix)
        aggregation = "      - name: verify exact non-overlapping aggregate\n"
        self.assertIn(aggregation, self.mutation_source)
        for setting in ("if: false", "continue-on-error: true"):
            with self.subTest(aggregate_setting=setting):
                changed = self.mutation_source.replace(aggregation,
                    aggregation + "        " + setting + "\n", 1)
                with self.assertRaises(AssertionError):
                    validate_mutation_storage(changed)
        for retention in ("retention-days: 30", "retention-days: 7", "retention-days: 1"):
            with self.subTest(mutation_retention=retention):
                changed = self.mutation_source.replace("retention-days: 2", retention, 1)
                self.assertNotEqual(changed, self.mutation_source)
                with self.assertRaises(AssertionError):
                    validate_mutation_storage(changed)
        for before, after in (
            ("name: mutation-manifest", "name: dropped-manifest"),
            ("name: mutation-shard-", "name: dropped-shard-"),
        ):
            with self.subTest(before=before):
                self.assertIn(before, self.mutation_source)
                with self.assertRaises(AssertionError):
                    validate_mutation_storage(self.mutation_source.replace(before, after, 1))


if __name__=="__main__":
    unittest.main()
