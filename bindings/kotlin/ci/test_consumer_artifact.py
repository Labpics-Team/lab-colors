"""Передача тех же APK без Gradle-состояния и подмены их происхождения."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import consumer_artifact as artifact


class ConsumerArtifactTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.consumer = self.root / "consumer"
        self.distribution = self.root / "sdk"
        self.destination = self.root / "transferred"
        self.source = {"sourceCommit": "a" * 40, "sourceTree": "b" * 40, "coreTree": "c" * 40}
        source = patch.object(artifact, "_source_identity", return_value=self.source)
        source.start()
        self.addCleanup(source.stop)
        outputs = self.consumer / "build/outputs"
        stages = {variant: {"app": "assembled", "test": "assembled"}
                  for variant in ("debug", "release")}
        self.write(outputs / "consumer-diagnostics/assembly-stages.json", json.dumps(stages))
        for variant in stages:
            self.write(outputs / f"apk/{variant}/app-{variant}.apk", f"SIGNED-{variant}-APPLICATION")
            self.write(outputs / f"apk/androidTest/{variant}/test-{variant}.apk", f"SIGNED-{variant}-TEST")
            for configuration in (variant + "RuntimeClasspath", variant + "AndroidTestRuntimeClasspath"):
                self.write(outputs / f"consumer-diagnostics/{variant}/{configuration}.txt", "dependency: exact version")
        for name in ("mapping.txt", "configuration.txt", "usage.txt", "seeds.txt"):
            self.write(outputs / "mapping/release" / name, "" if name in ("usage.txt", "seeds.txt") else "R8 result")
        self.write(outputs / "consumer-diagnostics/release/desugar-linkage.txt", "executable linkage resolved")
        self.write(self.consumer / "gradle-path.txt", "/foreign/absolute/gradle")
        self.write(self.consumer / ".gradle/cached-result", "not transferable")
        self.write(self.distribution / "LabColors.android.zip", "EXACT-SDK")
        manifest = {"schemaVersion": 1, "archive": "LabColors.android.zip",
                    "archiveSha256": artifact.digest(self.distribution / "LabColors.android.zip"),
                    "sourceCommit": self.source["sourceCommit"], "coreTree": self.source["coreTree"]}
        self.write(self.distribution / "android-release-manifest.json", json.dumps(manifest))

    @staticmethod
    def write(path, text):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def seal(self, destination=None):
        target = destination or self.destination
        artifact.seal(self.consumer, self.distribution, target)
        return target

    def edit_manifest(self, target, change):
        path = target / "manifest.json"
        value = json.loads(path.read_text())
        change(value)
        path.write_text(json.dumps(value))

    def test_same_signed_bytes_and_every_output_transfer_without_gradle_state(self):
        target = self.seal()
        manifest = artifact.verify_artifact(target)
        self.assertEqual({key: manifest[key] for key in self.source}, self.source)
        self.assertEqual(manifest["files"], artifact._files(self.consumer / "build/outputs"))
        for variant in ("debug", "release"):
            for name in (f"apk/{variant}/app-{variant}.apk", f"apk/androidTest/{variant}/test-{variant}.apk"):
                self.assertEqual((target / "outputs" / name).read_bytes(),
                                 (self.consumer / "build/outputs" / name).read_bytes())
        self.assertFalse((target / "gradle-path.txt").exists())
        self.assertFalse((target / ".gradle").exists())
        self.write(target / "runtime/debug/result.json", "fresh runtime observations")
        self.assertEqual(artifact.verify_artifact(target), manifest)

    def test_partial_variants_and_missing_required_artifacts_cannot_be_sealed(self):
        outputs = self.consumer / "build/outputs"
        missing = [f"apk/{variant}/app-{variant}.apk" for variant in ("debug", "release")]
        missing += ["apk/androidTest/release/test-release.apk", "mapping/release/mapping.txt",
                    "consumer-diagnostics/release/desugar-linkage.txt",
                    "consumer-diagnostics/debug/debugRuntimeClasspath.txt"]
        for index, name in enumerate(missing):
            with self.subTest(name=name):
                path = outputs / name
                previous = path.read_bytes()
                path.unlink()
                destination = self.root / ("missing-" + str(index))
                with self.assertRaises(ValueError):
                    self.seal(destination)
                self.assertFalse(destination.exists())
                path.write_bytes(previous)
        stages = outputs / "consumer-diagnostics/assembly-stages.json"
        previous = stages.read_text()
        value = json.loads(previous)
        value["release"]["test"] = "not-built"
        stages.write_text(json.dumps(value))
        with self.assertRaisesRegex(ValueError, "assembly did not complete"):
            self.seal()
        stages.write_text(previous)

    def test_only_one_application_and_test_apk_per_variant_is_accepted(self):
        extra = self.consumer / "build/outputs/apk/debug/second.apk"
        self.write(extra, "ANOTHER SIGNED APPLICATION")
        with self.assertRaisesRegex(ValueError, "one assembled"):
            self.seal()
        extra.unlink()
        target = self.seal()
        self.write(target / "outputs/apk/androidTest/release/second.apk", "another test")
        self.edit_manifest(target, lambda value: value.update(files=artifact._files(target / "outputs")))
        with self.assertRaisesRegex(ValueError, "one assembled"):
            artifact.verify_artifact(target)

    def test_empty_mapping_or_failed_dependency_evidence_cannot_pass(self):
        outputs = self.consumer / "build/outputs"
        failures = {"mapping/release/mapping.txt": "",
                    "consumer-diagnostics/debug/debugRuntimeClasspath.txt": "",
                    "consumer-diagnostics/release/releaseRuntimeClasspath.txt": "unresolved dependency FAILED"}
        for index, (name, content) in enumerate(failures.items()):
            with self.subTest(name=name):
                path = outputs / name
                previous = path.read_text()
                path.write_text(content)
                with self.assertRaises(ValueError):
                    self.seal(self.root / ("empty-" + str(index)))
                path.write_text(previous)

    def test_wrong_sdk_revision_core_or_archive_bytes_are_rejected(self):
        path = self.distribution / "android-release-manifest.json"
        original = json.loads(path.read_text())
        for field in ("sourceCommit", "coreTree", "archiveSha256"):
            with self.subTest(field=field):
                value = dict(original, **{field: "d" * len(original[field])})
                path.write_text(json.dumps(value))
                with self.assertRaises(ValueError):
                    self.seal(self.root / field)
        path.write_text(json.dumps(original))
        (self.distribution / "LabColors.android.zip").write_bytes(b"a different SDK")
        with self.assertRaisesRegex(ValueError, "SDK archive checksum"):
            self.seal()

    def test_wrong_tree_or_sdk_manifest_is_rejected_before_runtime(self):
        for field in ("sourceCommit", "sourceTree", "coreTree", "sdkManifestSha256", "sdkArchiveSha256"):
            with self.subTest(field=field):
                target = self.seal(self.root / field)
                self.edit_manifest(target, lambda value: value.update({field: "d" * len(value[field])}))
                with self.assertRaises(ValueError):
                    artifact.verify_artifact(target)
        target = self.seal()
        manifest = target / "sdk-manifest.json"
        value = json.loads(manifest.read_text())
        value["unadvertised-change"] = True
        manifest.write_text(json.dumps(value))
        with self.assertRaisesRegex(ValueError, "SDK manifest identity"):
            artifact.verify_artifact(target)

    def test_deleted_added_or_modified_output_is_rejected_on_readback(self):
        for mutation in ("deleted", "extra", "changed"):
            with self.subTest(mutation=mutation):
                target = self.seal(self.root / mutation)
                apk = target / "outputs/apk/debug/app-debug.apk"
                if mutation == "deleted":
                    apk.unlink()
                elif mutation == "extra":
                    self.write(target / "outputs/unadvertised.txt", "unexpected")
                else:
                    apk.write_bytes(b"different signed application")
                with self.assertRaisesRegex(ValueError, "inventory or checksum"):
                    artifact.verify_artifact(target)

    def test_noncanonical_inventory_and_duplicate_json_keys_are_rejected(self):
        for index, name in enumerate(("../outside", "/absolute", "apk\\debug", "apk//debug")):
            with self.subTest(name=name):
                target = self.seal(self.root / ("path-" + str(index)))
                self.edit_manifest(target, lambda value: value.update(files={name: "a" * 64}))
                with self.assertRaisesRegex(ValueError, "invalid.*inventory"):
                    artifact.verify_artifact(target)
        target = self.seal()
        path = target / "manifest.json"
        content = path.read_text()
        path.write_text(content.rstrip()[:-1] + ', "schemaVersion": 1}')
        with self.assertRaisesRegex(ValueError, "duplicate"):
            artifact.verify_artifact(target)

    def test_filesystem_links_cannot_replace_sealed_material(self):
        outputs = self.consumer / "build/outputs"
        outside = self.root / "outside"
        outside.mkdir()
        self.write(outside / "file", "other material")
        linked = outputs / "link"
        try:
            linked.symlink_to(outside, target_is_directory=True)
        except OSError as error:
            self.skipTest("symlink creation unavailable: " + str(error))
        with self.assertRaisesRegex(ValueError, "filesystem links"):
            self.seal()
        linked.unlink()
        target = self.seal()
        sdk = target / "sdk-manifest.json"
        sdk.unlink()
        sdk.symlink_to(self.distribution / "android-release-manifest.json")
        with self.assertRaisesRegex(ValueError, "regular file"):
            artifact.verify_artifact(target)

    def test_transfer_cannot_relabel_changed_outputs_as_verified(self):
        original = shutil.copytree

        def corrupted_copy(source, destination, *args, **kwargs):
            result = original(source, destination, *args, **kwargs)
            if Path(destination) == self.destination / "outputs":
                (Path(destination) / "apk/debug/app-debug.apk").write_bytes(b"a changed APK")
            return result

        with patch.object(artifact.shutil, "copytree", side_effect=corrupted_copy):
            with self.assertRaisesRegex(ValueError, "changed during transfer"):
                self.seal()
        self.assertFalse(self.destination.exists())

    def test_existing_destination_is_preserved(self):
        self.destination.mkdir()
        self.write(self.destination / "existing", "retained")
        with self.assertRaises(FileExistsError):
            self.seal()
        self.assertEqual((self.destination / "existing").read_text(), "retained")

    @unittest.skipIf(os.name == "nt" or (hasattr(os, "geteuid") and os.geteuid() == 0),
                     "directory permissions require a non-root POSIX process")
    def test_unreadable_directory_cannot_silently_shorten_output_inventory(self):
        target = self.seal()
        directory = target / "outputs/additional-evidence"
        self.write(directory / "report", "must be transferred")
        directory.chmod(0)
        try:
            with self.assertRaises(PermissionError):
                artifact.verify_artifact(target)
        finally:
            directory.chmod(0o700)


class SourceIdentityTests(unittest.TestCase):
    def test_git_revision_guard_rejects_changed_or_untracked_checked_out_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            core = root / "crates/labcolors-core/source.txt"
            core.parent.mkdir(parents=True)
            core.write_text("committed source")
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            subprocess.run(["git", "-C", str(root), "add", "."], check=True)
            subprocess.run(["git", "-C", str(root), "-c", "user.name=Artifact test", "-c", "user.email=artifact@example.invalid",
                            "commit", "-q", "-m", "source"], check=True)
            with patch.object(artifact, "ROOT", root):
                identity = artifact._source_identity()
                for sha in identity.values():
                    self.assertRegex(sha, "^[0-9a-f]{40}$")
                core.write_text("changed source")
                with self.assertRaisesRegex(ValueError, "revision must be clean"):
                    artifact._source_identity()
                core.write_text("committed source")
                (root / "new-input.txt").write_text("not in the checked Git tree")
                with self.assertRaisesRegex(ValueError, "revision must be clean"):
                    artifact._source_identity()


if __name__ == "__main__":
    unittest.main()
