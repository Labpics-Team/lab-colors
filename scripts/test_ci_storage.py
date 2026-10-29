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
    for guard in ["github.event_name == 'push'", "github.ref_type == 'tag'", "startsWith(github.ref_name, 'colors-v')"]:
        if guard not in uploads[0]:
            raise AssertionError("ordinary CI can retain the package")
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


def validate_native_storage(source):
    if source.count("uses: actions/upload-artifact@") != 1:
        raise AssertionError("native CI may only transfer the Android SDK")
    build = job(source, "android-build")
    blocks = re.findall(r"(?ms)^      - .*?(?=^      - |\Z)", build)
    upload = [block for block in blocks if "uses: actions/upload-artifact@" in block]
    if len(upload) != 1:
        raise AssertionError("Android SDK transfer missing")
    for required in ("name: android-sdk-", "retention-days: 1", "if-no-files-found: error"):
        if required not in upload[0]:
            raise AssertionError("Android SDK transfer changed: " + required)
    if "continue-on-error:" in upload[0] or "\n        if:" in upload[0]:
        raise AssertionError("Android SDK transfer became optional")
    consumer = job(source, "android-consumer")
    for required in ("needs: android-build", "uses: actions/download-artifact@",
                     "name: android-sdk-", "prepare-consumer.py", "test-consumer.py"):
        if required not in consumer:
            raise AssertionError("external Android consumer missing: " + required)
    for required in ("build-apple.py", "test-apple.py"):
        if required not in job(source, "apple-package"):
            raise AssertionError("external Apple verification missing: " + required)


class NativeStorageTests(unittest.TestCase):
    def setUp(self):
        self.source = (ROOT / ".github/workflows/native-conformance-worker.yml").read_text()

    def test_only_sdk_transfer_survives(self):
        validate_native_storage(self.source)

    def test_reintroduced_diagnostics_and_weakened_transfer_are_rejected(self):
        for before, after in [
            ("retention-days: 1", "retention-days: 7"),
            ("if-no-files-found: error", "if-no-files-found: ignore"),
            ("needs: android-build", "needs: other"),
            ("test-apple.py", "skip-apple.py"),
        ]:
            with self.subTest(before=before):
                self.assertIn(before, self.source)
                with self.assertRaises(AssertionError):
                    validate_native_storage(self.source.replace(before, after))
        with self.assertRaises(AssertionError):
            validate_native_storage(self.source + "\n      - uses: actions/upload-artifact@v4\n")


if __name__=="__main__":
    unittest.main()
