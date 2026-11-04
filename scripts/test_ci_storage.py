"""Локальные байты пакета и изоляция полномочий публикации."""
from pathlib import Path
import os
import tempfile
import json
import re
import subprocess
import textwrap
import unittest

ROOT=Path(__file__).resolve().parents[1]


def job(source, name):
    match=re.search(r"(?ms)^  "+re.escape(name)+r":\n.*?(?=^  [a-zA-Z][a-zA-Z0-9_-]*:|\Z)",source)
    if not match:
        raise AssertionError("job absent: "+name)
    return match.group()


def require_needs(source, name, expected):
    values = re.findall(r"(?m)^    needs: (.+)$", job(source, name))
    if len(values) != 1:
        raise AssertionError("job dependency is missing or repeated: " + name)
    try:
        observed = json.loads(values[0]) if values[0].startswith("[") else [values[0]]
    except ValueError as error:
        raise AssertionError("invalid job dependencies: " + name) from error
    if (not isinstance(observed, list)
            or not all(isinstance(value, str) for value in observed)
            or len(observed) != len(set(observed))
            or set(observed) != set(expected)):
        raise AssertionError("job bypasses required dependencies: " + name)


def require_permissions(source, name, expected):
    header = (job(source, name).split("    steps:", 1)[0] if name
              else source.split("\njobs:\n", 1)[0])
    indent = "    " if name else ""
    scopes = re.findall(r"(?m)^" + indent + r"permissions:.*$", header)
    if not scopes and name:
        return require_permissions(source, None, expected)
    blocks = re.findall(r"(?ms)^" + indent + r"permissions:([^\n]*)\n"
                        r"(.*?)(?=^" + indent + r"[^\s#]|\Z)", header)
    if len(scopes) != 1 or len(blocks) != 1 or blocks[0][0].strip():
        raise AssertionError("permission scope is missing or repeated: " + str(name))
    entries = []
    for line in blocks[0][1].splitlines():
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        match = re.fullmatch(indent + r"  ([a-z-]+): ([a-z]+)\s*", line)
        if not match:
            raise AssertionError("invalid permission mapping: " + str(name))
        entries.append(match.groups())
    observed = dict(entries)
    if len(entries) != len(observed) or observed != expected:
        raise AssertionError("job permissions changed: " + str(name))


def require_condition(source, name, expected):
    header = job(source, name).split("    steps:", 1)[0]
    matches = re.findall(r"(?ms)^    if: (.*?)(?=^    \S|\Z)", header)
    if len(matches) != 1:
        raise AssertionError("job condition is missing or repeated: " + name)
    condition = matches[0]
    if condition.startswith((">-\n", "|\n")):
        condition = condition.split("\n", 1)[1]
    if " ".join(line.strip() for line in condition.splitlines()) != expected:
        raise AssertionError("job condition can skip required execution: " + name)


def require_terminal_browser_environment(step):
    values = re.findall(r"(?m)^          BASH_ENV: ([^\r\n]+)$", step)
    if values not in (['""'], ["''"]):
        raise AssertionError("terminal browser must clear inherited BASH_ENV")


def validate(ci, package, release):
    for caller in [job(ci,"wasm"),job(release,"prepare")]:
        if "    uses: ./.github/workflows/package-worker.yml\n" not in caller:
            raise AssertionError("package preparation owner changed")
        if "secrets:" in caller or "environment:" in caller:
            raise AssertionError("preparation acquired publisher permissions")
    # YAML-полномочия проверяются отдельно от встроенного Python source admission.
    if re.search(r"(?m)^\s*(?:environment|secrets):", package):
        raise AssertionError("package checks acquired credentials")
    require_permissions(package, None, {"contents": "read"})
    for name in ("source-admission", "wasm"):
        require_permissions(package, name, {"contents": "read"})
        block = job(package, name)
        secret_refs = re.findall(r"secrets\.([A-Za-z0-9_]+)", block)
        if secret_refs and (name != "source-admission" or set(secret_refs) != {"GITHUB_TOKEN"}):
            raise AssertionError("package checks acquired credentials")
    admission = job(package, "source-admission")
    if "secrets." in package.replace(admission, "", 1):
        raise AssertionError("package checks acquired credentials")
    if job_level_optional(package, "source-admission"):
        raise AssertionError("package source admission became optional")
    require_needs(ci, "wasm", ["source-admission"])
    if job_level_optional(ci, "wasm"):
        raise AssertionError("package preparation caller became optional")
    require_needs(package, "wasm", ["source-admission"])
    if job_level_optional(package, "wasm"):
        raise AssertionError("package verification job became optional")
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
    browser_command = ('node scripts/test-program-runtime-browser.mjs '
                       '"$VERIFIED_TARBALL" "$VERIFIED_TARBALL_SHA256"')
    browser_steps = [block for block in blocks if browser_command in block]
    # Отдельный шаг с одной командой исключает ранний выход и маскировку её статуса.
    if len(browser_steps) != 1:
        raise AssertionError("terminal browser invocation changed")
    # Явный Bash перекрывает наследуемые defaults.run.shell.
    shells = re.findall(r"(?m)^        shell: ([^\r\n]+)$", browser_steps[0])
    if shells not in (["bash"], ["\"bash\""], ["'bash'"]):
        raise AssertionError("terminal browser shell does not execute the command")
    require_terminal_browser_environment(browser_steps[0])
    runs = re.findall(r"(?ms)^        run: (.*?)(?=^        \S|\Z)", browser_steps[0])
    statements = [[line.strip() for line in run.splitlines()
                   if line.strip() and not line.lstrip().startswith("#")]
                  for run in runs]
    if statements != [[browser_command]]:
        raise AssertionError("terminal browser invocation is not executed")
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
    require_needs(release, "prepare", ["source-admission"])
    require_needs(release, "publish", ["prepare", "source-admission"])
    require_needs(release, "verify-publication", ["prepare", "publish", "source-admission"])
    release_condition = ("github.repository == 'Labpics-Team/lab-colors' && "
                         "github.event_name == 'push' && github.ref_type == 'tag' && "
                         "startsWith(github.ref_name, 'colors-v')")
    for name in ("source-admission", "prepare", "publish"):
        require_condition(release, name, release_condition)
    require_permissions(release, None, {"contents": "read", "actions": "read"})
    for name in ("source-admission", "prepare", "verify-publication"):
        require_permissions(release, name, {"contents": "read"})
        if re.search(r"(?m)^\s*(?:environment|secrets):", job(release, name)):
            raise AssertionError("preparation acquired publisher permissions")
    require_permissions(release, "publish",
                        {"contents": "read", "actions": "read", "id-token": "write"})
    if ("    environment: npm-publish\n" not in publisher
            or re.search(r"(?m)^    continue-on-error:", publisher)):
        raise AssertionError("publish bypasses isolated verification")
    if job_level_optional(release, "verify-publication"):
        raise AssertionError("public package verification became optional")
    if "run-id:" in publisher or "steps.canonical-runs.outputs.ci_run_attempt" in publisher:
        raise AssertionError("release requires a retained ordinary CI archive")


class CiStorageTests(unittest.TestCase):
    def setUp(self):
        self.sources=[(ROOT/".github/workflows"/name).read_text() for name in
                      ["ci-worker.yml","package-worker.yml","publish-worker.yml"]]

    def test_current_package_graph_retains_only_the_release_handoff(self):
        validate(*self.sources)

    def test_terminal_browser_shell_overrides_inherited_defaults(self):
        browser = ('node scripts/test-program-runtime-browser.mjs '
                   '"$VERIFIED_TARBALL" "$VERIFIED_TARBALL_SHA256"')
        marker = '        shell: bash\n        run: ' + browser
        self.assertEqual(self.sources[1].count(marker), 1)
        for shell in ('true {0}', 'python', ''):
            values = list(self.sources)
            replacement = (('        shell: ' + shell + '\n') if shell else '')
            replacement += '        run: ' + browser
            values[1] = values[1].replace(marker, replacement, 1)
            with self.subTest(shell=shell), self.assertRaises(AssertionError):
                validate(*values)
        for shell in ('"bash"', "'bash'"):
            values = list(self.sources)
            values[1] = values[1].replace(marker,
                '        shell: ' + shell + '\n        run: ' + browser, 1)
            validate(*values)
        for scope in ('workflow', 'job'):
            values = list(self.sources)
            if scope == 'workflow':
                values[1] = values[1].replace('\njobs:\n',
                    '\ndefaults:\n  run:\n    shell: true {0}\njobs:\n', 1)
            else:
                values[1] = values[1].replace('  wasm:\n',
                    '  wasm:\n    defaults:\n      run:\n        shell: true {0}\n', 1)
            # Небезопасный default не применяется к явно выбранному Bash.
            validate(*values)

    def test_read_only_native_token_is_confined_to_source_admission(self):
        values = list(self.sources)
        admission = job(values[1], "source-admission")
        marker = "        env:\n"
        self.assertEqual(admission.count(marker), 1)
        token_line = "          GH_TOKEN: $" + "{{ secrets.GITHUB_TOKEN }}\n"
        changed = admission.replace(marker, marker + token_line, 1)
        values[1] = values[1].replace(admission, changed, 1)
        validate(*values)
        for before, after in (
            ("      contents: read", "      contents: write"),
            ("    permissions:\n      contents: read", "    permissions: write-all"),
            ("    permissions:\n      contents: read",
             "    permissions:\n      contents: read\n      id-token: write"),
            ("    permissions:\n      contents: read",
             "    permissions:\n      contents: read\n    # комментарий не закрывает mapping\n      id-token: write"),
            ("secrets.GITHUB_TOKEN", "secrets.NPM_TOKEN"),
        ):
            with self.subTest(before=before):
                altered = list(values)
                self.assertIn(before, changed)
                altered[1] = values[1].replace(changed, changed.replace(before, after, 1), 1)
                with self.assertRaises(AssertionError):
                    validate(*altered)
        altered = list(self.sources)
        owner = job(altered[1], "wasm")
        altered[1] = altered[1].replace(owner, owner.replace(
            "  wasm:\n", "  wasm:\n    env:\n" + token_line, 1), 1)
        with self.assertRaises(AssertionError):
            validate(*altered)

    def test_required_admission_and_publish_execution_cannot_be_skipped(self):
        # Порядок зависимостей в YAML не задаёт порядок выполнения задач.
        for name in ("publish", "verify-publication"):
            values = list(self.sources)
            block = job(values[2], name)
            match = re.search(r"(?m)^    needs: (.+)$", block)
            dependencies = json.loads(match.group(1))
            changed = block.replace(match.group(),
                                    "    needs: " + json.dumps(list(reversed(dependencies))), 1)
            values[2] = values[2].replace(block, changed, 1)
            validate(*values)
            for invalid in (dependencies[1:], dependencies + dependencies[:1], dependencies + [1]):
                altered = list(self.sources)
                changed = block.replace(match.group(), "    needs: " + json.dumps(invalid), 1)
                altered[2] = altered[2].replace(block, changed, 1)
                with self.subTest(job=name, dependencies=invalid), self.assertRaises(AssertionError):
                    validate(*altered)
        for index, name in ((0, "wasm"), (1, "source-admission"), (1, "wasm"),
                            (2, "verify-publication")):
            for setting in ("if: false", "continue-on-error: true"):
                with self.subTest(index=index, job=name, setting=setting):
                    values = list(self.sources)
                    block = job(values[index], name)
                    values[index] = values[index].replace(block,
                        block.replace("  " + name + ":\n",
                                      "  " + name + ":\n    " + setting + "\n", 1), 1)
                    with self.assertRaises(AssertionError):
                        validate(*values)
        for name in ("source-admission", "prepare", "publish"):
            values = list(self.sources)
            block = job(values[2], name)
            self.assertIn("    if: >-\n", block)
            values[2] = values[2].replace(block, block.replace(
                "    if: >-\n", "    if: >-\n      false &&\n", 1), 1)
            with self.subTest(job=name), self.assertRaises(AssertionError):
                validate(*values)
        for name in ("source-admission", "prepare", "verify-publication"):
            values = list(self.sources)
            block = job(values[2], name)
            self.assertIn("      contents: read", block)
            values[2] = values[2].replace(block, block.replace(
                "      contents: read", "      contents: read\n      id-token: write", 1), 1)
            with self.subTest(job=name), self.assertRaises(AssertionError):
                validate(*values)
        values = list(self.sources)
        publisher = job(values[2], "publish")
        values[2] = values[2].replace(publisher, publisher.replace(
            "      actions: read", "      actions: write", 1), 1)
        with self.assertRaises(AssertionError):
            validate(*values)

    def test_skipped_floor_external_input_and_changed_permissions_are_rejected(self):
        browser = ('node scripts/test-program-runtime-browser.mjs '
                   '"$VERIFIED_TARBALL" "$VERIFIED_TARBALL_SHA256"')
        for index,before,after in [
            (1, browser, "printf '%s\\n' 'node scripts/test-program-runtime-browser.mjs'"),
            (1, browser, '# ' + browser),
            (1, '        run: ' + browser,
             '        run: |\n          exit 0\n          ' + browser),
            (1, '        run: ' + browser,
             '        run: |\n          if false; then\n          ' + browser + '\n          fi'),
            (1, browser, 'false && ' + browser),
            (1, browser, 'trap \'exit 0\' EXIT; ' + browser),
            (1, browser, browser + ' || true'),
            (1, browser, browser + '\n          || true'),
            (1, browser, browser + '\n          && false || true'),
            (1, browser, 'if false; then ' + browser + '; fi'),
            (1, browser, browser.replace('$VERIFIED_TARBALL_SHA256', '$OTHER_SHA256')),
            (0,'    uses: ./.github/workflows/package-worker.yml','    uses: ./.github/workflows/other.yml'),
            (1,'npm run release:verify','npm --version'),
            (1,'npm run typecheck','echo typecheck'),
            (1,'node scripts/test-program-runtime-browser.mjs','echo browser'),
            (1,'      - name: terminal Program in real browser',
             '      - name: terminal Program in real browser\n        if: false'),
            (1,'      - name: terminal Program in real browser',
             '      - name: terminal Program in real browser\n        continue-on-error: true'),
            (1,'--package-smoke "$VERIFIED_TARBALL"','--help'),
            (1,"github.ref_type == 'tag'","true"),
            (1,"if: github.event_name == 'push'", "if: false && github.event_name == 'push'"),
            (1,"retention-days: 1", "retention-days: 1\n        continue-on-error: true"),
            (1,'retention-days: 1','retention-days: 90'),
            (2,'    needs: ["prepare", "source-admission"]','    needs: ["source-admission"]'),
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
    if job_level_optional(source, "source-admission"):
        raise AssertionError("native source admission became optional")
    if source.count("uses: actions/upload-artifact@") != 1:
        raise AssertionError("native CI may only transfer the Android SDK")
    require_needs(source, "android-build", ["source-admission"])
    require_needs(source, "android-consumer", ["android-build", "source-admission"])
    require_needs(source, "apple-package", ["source-admission"])
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
    for required in ("uses: actions/download-artifact@",
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
    if job_level_optional(source, "source-admission"):
        raise AssertionError("mutation source admission became optional")
    # Shards and their manifest cross job boundaries; the aggregate report
    # already appears in job logs and must never consume artifact quota.
    if source.count("uses: actions/upload-artifact@") != 2:
        raise AssertionError("mutation CI may only transfer its manifest and shards")
    manifest = job(source, "manifest")
    shard = job(source, "shard")
    aggregate = job(source, "aggregate")
    require_needs(source, "manifest", ["source-admission"])
    require_needs(source, "shard", ["manifest", "source-admission"])
    require_needs(source, "aggregate", ["manifest", "shard", "source-admission"])
    for name in ("manifest", "shard"):
        if job_level_optional(source, name):
            raise AssertionError("mutation input job became optional: " + name)
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
    require_condition(source, "aggregate",
                      "$" + "{{ (always()) && needs.source-admission.result == 'success' }}")
    if re.search(r"(?m)^    continue-on-error:", aggregate):
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
        for source, validator in ((self.source, validate_native_storage),
                                  (self.mutation_source, validate_mutation_storage)):
            for setting in ("if: false", "continue-on-error: true"):
                marker = "  source-admission:\n"
                self.assertIn(marker, source)
                changed = source.replace(marker, marker + "    " + setting + "\n", 1)
                with self.subTest(admission_setting=setting), self.assertRaises(AssertionError):
                    validator(changed)
        for before, after in [
            ("retention-days: 1", "retention-days: 7"),
            ("retention-days: 1", "retention-days: 14"),
            ("if-no-files-found: error", "if-no-files-found: ignore"),
            ('needs: ["android-build", "source-admission"]', 'needs: ["source-admission"]'),
            ("test-apple.py", "skip-apple.py"),
            ("script: python3 bindings/kotlin/ci/test-consumer.py", "script: echo skip-runtime"),
            ('run: python3 bindings/kotlin/ci/test-consumer.py "$RUNNER_TEMP/android-consumer" --assemble',
             'run: echo skip-external-assembly'),
        ]:
            with self.subTest(before=before):
                self.assertIn(before, self.source)
                with self.assertRaises(AssertionError):
                    validate_native_storage(self.source.replace(before, after))
        for name in ("manifest", "shard", "aggregate"):
            original = job(self.mutation_source, name)
            dependency = re.search(r"(?m)^    needs: (.+)$", original)
            self.assertIsNotNone(dependency)
            changed = original.replace(dependency.group(),
                                       "    needs: [\"source-admission\"]", 1)
            if name == "manifest":
                changed = original.replace(dependency.group(), "    needs: []", 1)
            with self.subTest(mutation_dependency=name), self.assertRaises(AssertionError):
                validate_mutation_storage(self.mutation_source.replace(original, changed, 1))
        for name in ("manifest", "shard"):
            original = "  " + name + ":\n"
            for setting in ("if: false", "continue-on-error: true"):
                with self.subTest(mutation_job=name, setting=setting), self.assertRaises(AssertionError):
                    validate_mutation_storage(self.mutation_source.replace(
                        original, original + "    " + setting + "\n", 1))
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
        aggregate_condition = "    if: $" + "{{ (always()) && needs.source-admission.result == 'success' }}\n"
        aggregate_job = "  aggregate:\n    name: prove complete mutation population\n" + aggregate_condition
        self.assertIn(aggregate_job, self.mutation_source)
        for replacement in (
                aggregate_job.replace(aggregate_condition, "    if: false\n"),
                aggregate_job.replace(aggregate_condition, "    if: always()\n"),
                aggregate_job.replace(aggregate_condition,
                    "    if: $" + "{{ success() && needs.source-admission.result == 'success' }}\n"),
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


keys = ('ALSA_PACKAGE', 'ATK_PACKAGE', 'ATK_BRIDGE_PACKAGE', 'ATSPI_PACKAGE', 'CUPS_PACKAGE', 'GLIB_PACKAGE')
old = ('libasound2', 'libatk1.0-0', 'libatk-bridge2.0-0', 'libatspi2.0-0', 'libcups2', 'libglib2.0-0')
time64 = tuple(value + 't64' for value in old)

class ChromeDependencyReleaseTests(unittest.TestCase):
    def evaluate(self, distro, release):
        source = (ROOT / ".github/workflows/package-worker.yml").read_text()
        match = re.search(r'(?ms)^\s*case "\$ID:\$VERSION_CODENAME" in\n.*?^\s*esac$', source)
        self.assertIsNotNone(match, "Chrome ABI selector is missing")
        selector = textwrap.dedent(match.group())
        output = '\n'.join('printf "%s\\n" "$' + key + '"' for key in keys)
        return subprocess.run(['bash', '-eu', '-c', selector + '\n' + output],
                              env={'PATH': '/usr/bin:/bin', 'ID': distro, 'VERSION_CODENAME': release},
                              text=True, capture_output=True, timeout=5)

    def test_legacy_and_time64_supported_releases(self):
        for distro, release, expected in (
            ('debian', 'bookworm', old), ('ubuntu', 'jammy', old),
            ('debian', 'trixie', time64), ('ubuntu', 'noble', time64),
            ('ubuntu', 'resolute', time64),
        ):
            with self.subTest(distro=distro, release=release):
                result = self.evaluate(distro, release)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(tuple(result.stdout.splitlines()), expected)

    def test_unknown_release_cannot_select_a_plausible_abi(self):
        for distro, release in (('ubuntu', 'future'), ('debian', 'resolute'), ('alpine', 'noble')):
            with self.subTest(distro=distro, release=release):
                result = self.evaluate(distro, release)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, '')
                self.assertIn('unsupported Chrome dependency release: ' + distro + ':' + release, result.stderr)

class BrowserStartupEnvironmentTests(unittest.TestCase):
    def step(self):
        source = (ROOT / ".github/workflows/package-worker.yml").read_text()
        return next(block for block in re.findall(r"(?ms)^      - .*?(?=^      - |\Z)", job(source, "wasm"))
                    if 'run: node scripts/test-program-runtime-browser.mjs ' in block)

    def execute(self, step):
        command = re.search(r"(?m)^        run: (.+)$", step).group(1)
        with tempfile.TemporaryDirectory(prefix="browser-shell-control-") as name:
            root = Path(name)
            startup = root / "startup.sh"
            startup.write_text("exit 0\n")
            trace = root / "argv"
            node = root / "node"
            node.write_text('#!/bin/bash\nprintf "%s\\n" "$@" > "$TRACE_FILE"\nexit 23\n')
            node.chmod(0o700)
            environment = dict(os.environ, PATH=str(root) + ":/usr/bin:/bin",
                               BASH_ENV=str(startup), TRACE_FILE=str(trace),
                               VERIFIED_TARBALL="exact-package.tgz", VERIFIED_TARBALL_SHA256="a" * 64)
            if re.search(r"(?m)^          BASH_ENV: (?:\"\"|'')$", step):
                environment["BASH_ENV"] = ""
            result = subprocess.run(["bash", "--noprofile", "--norc", "-e", "-o", "pipefail", "-c", command],
                                    env=environment, text=True, capture_output=True, timeout=5)
            return result.returncode, trace.read_text().splitlines() if trace.exists() else []

    def test_inherited_startup_cannot_skip_browser_or_erase_failure(self):
        code, arguments = self.execute(self.step())
        self.assertEqual(code, 23)
        self.assertEqual(arguments, ["scripts/test-program-runtime-browser.mjs", "exact-package.tgz", "a" * 64])

    def test_removing_empty_environment_is_detected_and_reproduces_skip(self):
        step = self.step()
        require_terminal_browser_environment(step)
        mutant = re.sub(r"(?m)^          BASH_ENV: .*\n", "", step)
        with self.assertRaisesRegex(AssertionError, "BASH_ENV"):
            require_terminal_browser_environment(mutant)
        self.assertEqual(self.execute(mutant), (0, []))


if __name__=="__main__":
    unittest.main()
