#!/usr/bin/env python3
"""Реальный Git → Cargo cache → public Core issuance/admission, без workspace build."""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import signal
import stat
import struct
import subprocess
import sys
import tempfile
import time
import traceback
import unittest
from pathlib import Path
from unittest import mock


REPO = Path(__file__).resolve().parent.parent
CORE = Path("crates/labcolors-core")
OUTPUT = "certificate-source-tree-v1.bin"
RUNTIME_SOURCES = ("src/certificate.rs", "src/certificate/source.rs", "src/sha256.rs")
BUILD_SOURCES = ("build.rs", "build_support/source_descriptor.rs")


def run_owned(args: list[str], *, cwd: Path, env: dict[str, str],
              timeout: float = 90) -> subprocess.CompletedProcess[str]:
    """Один command владеет fresh process group, включая Cargo/rustc/Git descendants."""
    process = subprocess.Popen(args, cwd=cwd, env=env, text=True,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               start_new_session=True)
    primary: BaseException | None = None
    completed: subprocess.CompletedProcess[str] | None = None
    try:
        stdout, stderr = process.communicate(timeout=timeout)
        completed = subprocess.CompletedProcess(args, process.returncode, stdout, stderr)
        return completed
    except BaseException as error:
        primary = error
        raise
    finally:
        cleanup_errors: list[tuple[str, BaseException]] = []
        # Убираем descendants и после обычного exit непосредственного child.
        # PID/PGID взят только из собственного fresh Popen, не из process search.
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        except BaseException as error:
            cleanup_errors.append(("group stop", error))
            try:
                process.kill()
            except ProcessLookupError:
                pass
            except BaseException as error:
                cleanup_errors.append(("direct child stop", error))
        try:
            process.communicate(timeout=5)
        except BaseException as error:
            cleanup_errors.append(("command reap", error))
        if cleanup_errors:
            had_primary = primary is not None
            cleanup = cleanup_errors[0][1]
            if primary is None:
                if completed is None or completed.returncode == 0:
                    primary = cleanup
                else:
                    primary = subprocess.CalledProcessError(completed.returncode, args,
                                                            completed.stdout, completed.stderr)
            # Три известные cleanup стадии, каждая до 1024 diagnostic characters.
            # Все детали идут прямо к primary: repr(exception) теряет __notes__.
            for phase, error in cleanup_errors:
                primary.add_note(f"secondary {phase} failure: {repr(error)[:1024]}")
            if not had_primary:
                if primary is cleanup:
                    raise primary
                raise primary from cleanup


def install_interrupt_handlers() -> None:
    def interrupted(signum: int, _frame: object) -> None:
        raise TimeoutError(f"test harness interrupted by signal {signum}")

    # Python владеет deadline и cleanup до внешнего CI timeout.
    for signum in (signal.SIGALRM, signal.SIGTERM, signal.SIGINT):
        signal.signal(signum, interrupted)


MAIN = r'''
use labcolors_core::certificate::{
    issue_source_certificate_v1, AdmissionKeyV1, AdmissionOutcomeV1, AdmissionStateV1,
    CertificateErrorV1, CertificateOperationV1, CertificateProducerErrorV1, UntrustedEnvelopeV1,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["unavailable"] {
        let error = issue_source_certificate_v1().unwrap_err();
        assert_eq!(error, CertificateProducerErrorV1::ProducerIdentityUnavailable);
        assert_eq!(error.code(), "producer_identity_unavailable");
        assert_eq!(format!("{error:?}"), "producer_identity_unavailable");
        assert_eq!(format!("{error}"), "producer_identity_unavailable");
        println!("unavailable");
        return;
    }
    assert_eq!(args.len(), 4);
    let content: Vec<u8> = args[1].as_bytes().chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap()).collect();
    // Expected tuple поступает из отдельного Git/hashlib oracle, не producer getters.
    let expected = AdmissionKeyV1::try_new(
        &args[2], CertificateOperationV1::IssueCertificate, &args[3], &args[0],
        content.try_into().unwrap(),
    ).unwrap();
    let issued = issue_source_certificate_v1().unwrap();
    assert_eq!(issued.admission_key(), &expected);
    let decoded = UntrustedEnvelopeV1::decode(issued.as_bytes()).unwrap();
    let mut state = AdmissionStateV1::new();
    assert_eq!(state.admit(&decoded, &expected, None),
        Err(CertificateErrorV1::MissingProducerAttestation));
    assert!(state.is_empty());
    assert_eq!(state.admit(&decoded, &expected, Some(issued.attestation())),
        Ok(AdmissionOutcomeV1::Accepted));
    let prior_bytes = state.accounted_bytes();
    assert_eq!(state.admit(&decoded, &expected, Some(issued.attestation())),
        Ok(AdmissionOutcomeV1::DuplicateNoop));
    assert_eq!(state.len(), 1);
    assert_eq!(state.accounted_bytes(), prior_bytes);
    for byte in issued.as_bytes() { print!("{byte:02x}"); }
    println!();
}
'''


def oracle(tree: str) -> tuple[bytes, list[str], bytes]:
    """r13 wire + source descriptor law, независимо от Rust encoder/getters."""
    descriptor = b"LCST" + struct.pack(">HB", 1, 1) + tree.encode("ascii")
    content = hashlib.sha256(b"labpics.colors/core-source-tree-descriptor/v1\0" + descriptor).digest()
    runtime = b"labcolors-core:source-tree-v1:" + content.hex().encode("ascii")
    context = b"core-source-tree-transport-v1"

    def text(value: bytes) -> bytes:
        return struct.pack(">H", len(value)) + value

    prefix = (
        b"LCEN" + struct.pack(">HBBH", 1, 1, 0, 1) + text(runtime)
        + text(tree.encode("ascii")) + content + text(context)
        + struct.pack(">BHI", 1, 1, 1) + b"\x01"
        + hashlib.sha256(b"labpics.colors/certificate-payload/v1\0\x01").digest()
    )
    envelope = prefix + hashlib.sha256(b"labpics.colors/certificate-envelope/v1\0" + prefix).digest()
    return descriptor, [tree, content.hex(), runtime.decode(), context.decode()], envelope


class SourceBuildTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="certificate-source-build-")
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.root = self.base / "repository"
        self.target = self.base / "target"
        self.checkout_targets: dict[Path, Path] = {}
        self.env = dict(os.environ)
        # Fixture — native std-only build, независимый от WASM job target/flags/cache.
        for name in list(self.env):
            if name.startswith("GIT_") and name != "GIT_EXEC_PATH":
                del self.env[name]
        for name in ("CARGO_BUILD_TARGET", "CARGO_ENCODED_RUSTFLAGS", "RUSTFLAGS", "CARGO_TARGET_DIR"):
            self.env.pop(name, None)
        self.env.update({
            "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": os.devnull,
            "CARGO_TERM_COLOR": "never", "CARGO_BUILD_JOBS": "1",
            "CARGO_INCREMENTAL": "0", "CARGO_NET_OFFLINE": "true",
        })
        self.make_source(self.root)
        self.init(self.root)

    def command(self, args: list[str], *, cwd: Path | None = None, env: dict[str, str] | None = None,
                check: bool = True) -> subprocess.CompletedProcess[str]:
        result = run_owned(args, cwd=cwd or self.root, env=env or self.env)
        if check and result.returncode:
            self.fail(f"command failed ({result.returncode}): {args}\n{result.stdout}\n{result.stderr}")
        return result

    def git(self, *args: str, root: Path | None = None) -> str:
        # Fixture commands must finish all Git writes before metadata snapshots.
        # Detached auto-maintenance would outlive their owned process group.
        return self.command(["git", "-c", "maintenance.auto=false", "-C",
                             str(root or self.root), *args]).stdout.strip()

    def make_source(self, root: Path) -> None:
        core = root / CORE
        core.mkdir(parents=True)
        (root / "Cargo.toml").write_text('[workspace]\nmembers=["crates/labcolors-core"]\nresolver="2"\n')
        (root / ".gitignore").write_text("/target/\n/crates/labcolors-core/ignored-input\n")
        (root / "LICENSE").write_text("license target outside selected subtree\n")
        (core / "Cargo.toml").write_text('[package]\nname="labcolors-core"\nversion="0.0.0"\nedition="2024"\n')
        for relative in (*BUILD_SOURCES, *RUNTIME_SOURCES):
            target = core / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(REPO / CORE / relative, target)
        (core / "src/lib.rs").write_text("pub mod certificate;\nmod sha256;\n")
        (core / "src/main.rs").write_text(MAIN)
        (core / "assets").mkdir()
        (core / "assets/input.txt").write_bytes(b"raw source artifact bytes\n")
        (core / "LICENSE").symlink_to("../../LICENSE")
        (core / "another-link").symlink_to("does-not-exist")

    def init(self, root: Path, object_format: str = "sha1") -> None:
        self.git("init", "--quiet", "--initial-branch=main", f"--object-format={object_format}", root=root)
        self.git("config", "user.name", "Source descriptor fixture", root=root)
        self.git("config", "user.email", "source-descriptor@example.invalid", root=root)
        self.git("config", "core.fileMode", "true", root=root)
        # Lockfile существует уже в A: Cargo не создаёт untracked conflict при A/B checkout.
        self.command(["cargo", "generate-lockfile", "--offline", "--manifest-path",
                      str(root / CORE / "Cargo.toml")], cwd=root)
        self.commit(root=root)

    def commit(self, *, root: Path | None = None) -> str:
        self.git("add", "-A", root=root)
        self.git("commit", "--quiet", "--no-gpg-sign", "-m", "source build fixture", root=root)
        return self.git("rev-parse", "HEAD", root=root)

    def build(self, *, root: Path | None = None, available: bool = True,
              env: dict[str, str] | None = None, core: Path | None = None,
              default_target: bool = False) -> tuple[bytes, Path]:
        root = root or self.root
        core = core or root / CORE
        args = [
            "cargo", "build", "--offline", "--quiet", "--message-format=json",
            "--manifest-path", str(core / "Cargo.toml"),
        ]
        if not default_target:
            # Один постоянный cache на checkout во всех его историях. Cargo
            # может alias целый executable разных checkout в общем target.
            checkout = root.resolve()
            if checkout not in self.checkout_targets:
                self.checkout_targets[checkout] = self.target / str(len(self.checkout_targets))
            args.extend(["--target-dir", str(self.checkout_targets[checkout])])
        result = self.command(args, cwd=root, env=env)
        messages = [json.loads(line) for line in result.stdout.splitlines()]
        outputs = [Path(m["out_dir"]) for m in messages if m["reason"] == "build-script-executed"]
        executables = [m["executable"] for m in messages
                       if m["reason"] == "compiler-artifact" and m.get("executable")]
        self.assertEqual(len(outputs), 1, result.stdout)
        self.assertEqual(len(executables), 1, result.stdout)
        output = outputs[0] / OUTPUT
        if available:
            tree = self.git("rev-parse", f"HEAD:{CORE.as_posix()}", root=root)
            expected_descriptor, expected_key, expected_envelope = oracle(tree)
            self.assertEqual(output.read_bytes(), expected_descriptor)
            actual = self.command([executables[0], *expected_key], cwd=root).stdout.strip()
            self.assertEqual(bytes.fromhex(actual), expected_envelope)
            self.assertEqual(len(expected_envelope), 283)
        else:
            self.assertEqual(output.read_bytes(), b"", "stale valid descriptor survived refusal")
            actual = self.command([executables[0], "unavailable"], cwd=root).stdout.strip()
            self.assertEqual(actual, "unavailable")
        self.assertEqual(list(outputs[0].glob(f"{OUTPUT}.*.tmp")), [])
        return output.read_bytes(), output

    def test_cache_raw_source_drift_and_outside_commits(self) -> None:
        metadata = self.root / ".git"
        before_git = {str(p.relative_to(metadata)): (p.read_bytes(), p.stat().st_mtime_ns)
                      for p in metadata.rglob("*") if p.is_file()}
        original, output = self.build()
        after_git = {str(p.relative_to(metadata)): (p.read_bytes(), p.stat().st_mtime_ns)
                     for p in metadata.rglob("*") if p.is_file()}
        self.assertEqual(after_git, before_git, "build owner wrote Git metadata/objects")
        timestamp = output.stat().st_mtime_ns
        a = self.git("rev-parse", "HEAD")
        (self.root / "budget.json").write_text('{"measurement": 1}\n')
        self.commit()
        self.assertEqual(self.build()[0], original)
        self.assertEqual(output.stat().st_mtime_ns, timestamp, "same T rewrote output")
        source = self.root / CORE / "src/main.rs"
        clean = source.read_bytes()
        for flag in ("assume-unchanged", "skip-worktree"):
            with self.subTest(flag=flag):
                self.git("update-index", f"--{flag}", f"{CORE}/src/main.rs")
                source.write_bytes(clean + b"\n// uncommitted raw source drift\n")
                self.git("diff", "--exit-code", "--", f"{CORE}/src/main.rs")
                self.build(available=False)
                source.write_bytes(clean)
                self.assertEqual(self.build()[0], original)
                self.git("update-index", f"--no-{flag}", f"{CORE}/src/main.rs")
        source.write_bytes(clean + b"\n// committed source B\n")
        b = self.commit()
        changed, _ = self.build()
        self.assertNotEqual(changed, original)
        self.git("checkout", "--quiet", "--detach", a)
        self.assertEqual(self.build()[0], original)
        self.git("checkout", "--quiet", "--detach", b)
        self.assertEqual(self.build()[0], changed)

    def test_warm_chmod_both_directions_and_committed_modes(self) -> None:
        source = self.root / CORE / "src/main.rs"
        original, output = self.build()
        for physical, committed_mode in ((0o755, "100644"), (0o644, "100755")):
            with self.subTest(physical=oct(physical)):
                before, _ = self.build()
                modified = source.stat().st_mtime_ns
                source.chmod(physical)
                self.assertEqual(stat.S_IMODE(source.stat().st_mode), physical)
                self.assertEqual(source.stat().st_mtime_ns, modified)
                self.assertIn(committed_mode, self.git("ls-tree", "HEAD", f"{CORE}/src/main.rs"))
                timestamp = output.stat().st_mtime_ns
                self.assertEqual(self.build()[0], before)
                # Реальное metadata изменение повторяет verifier, не меняя T.
                self.git("config", "fixture.recheck", str(physical))
                self.assertEqual(self.build()[0], before)
                self.assertEqual(output.stat().st_mtime_ns, timestamp)
                self.commit()
                self.assertNotEqual(self.build()[0], before)
        self.assertEqual(self.build()[0], original)

    def test_symlinks_are_literal_entries_not_followed_targets(self) -> None:
        original, _ = self.build()
        link = self.root / CORE / "LICENSE"
        self.assertEqual(os.readlink(link), "../../LICENSE")
        (self.root / "LICENSE").write_bytes(b"changed outside target\n")
        self.assertEqual(self.build()[0], original)
        link.unlink()
        link.symlink_to("../../OTHER-LICENSE")
        self.build(available=False)
        self.commit()
        changed, _ = self.build()
        self.assertNotEqual(changed, original)
        link.unlink()
        link.write_bytes(b"../../OTHER-LICENSE")
        self.build(available=False)
        link.unlink()
        link.symlink_to("../../OTHER-LICENSE")
        self.assertEqual(self.build()[0], changed)

    def test_missing_extra_filtered_and_unsupported_entries(self) -> None:
        original, _ = self.build()
        core = self.root / CORE
        source = core / "assets/input.txt"
        clean = source.read_bytes()
        source.unlink()
        self.build(available=False)
        source.write_bytes(clean)
        self.assertEqual(self.build()[0], original)
        for name in ("extra-input", "ignored-input"):
            extra = core / name
            extra.write_bytes(b"untracked\n")
            self.build(available=False)
            extra.unlink()
            self.assertEqual(self.build()[0], original)
        empty = core / "extra-directory"
        empty.mkdir()
        self.build(available=False)
        empty.rmdir()
        source.unlink()
        source.symlink_to("../../../../outside-input")
        self.build(available=False)
        source.unlink()
        os.mkfifo(source)
        self.build(available=False)
        source.unlink()
        source.write_bytes(clean)
        (self.root / ".gitattributes").write_text(f"{CORE}/assets/input.txt text eol=lf\n")
        source.write_bytes(clean.replace(b"\n", b"\r\n"))
        self.commit()
        self.build(available=False)
        source.write_bytes(clean)
        self.build()
        commit = self.git("rev-parse", "HEAD")
        (core / "submodule").mkdir()
        self.git("update-index", "--add", "--cacheinfo", f"160000,{commit},{CORE}/submodule")
        self.git("commit", "--quiet", "--no-gpg-sign", "-m", "unsupported gitlink")
        self.build(available=False)

    def test_gitless_cache_refusal_and_metadata_reappearance(self) -> None:
        original, _ = self.build()
        metadata = self.root / ".git"
        held = self.base / "held-git-metadata"
        metadata.rename(held)
        self.build(available=False)
        self.build(available=False)
        held.rename(metadata)
        self.assertEqual(self.build()[0], original)
        linked = self.base / "linked-metadata"
        self.git("worktree", "add", "--quiet", "--detach", str(linked), "HEAD")
        self.assertEqual(self.build(root=linked)[0], original)
        git_dir = Path((linked / ".git").read_text().removeprefix("gitdir: ").strip())
        git_dir.rename(held)
        self.build(root=linked, available=False)
        held.rename(git_dir)
        self.assertEqual(self.build(root=linked)[0], original)
        blob = self.git("rev-parse", f"HEAD:{CORE}/assets/input.txt")
        obj = metadata / "objects" / blob[:2] / blob[2:]
        saved_blob = self.base / "held-blob"
        obj.rename(saved_blob)
        self.build(root=linked, available=False)
        saved_blob.rename(obj)
        self.assertEqual(self.build(root=linked)[0], original)
        # Архив без workspace использует обычный target внутри crate: сборка
        # работает, а public producer возвращает typed unavailable.
        archive = self.base / "archive/labcolors-core"
        shutil.copytree(self.root / CORE, archive, symlinks=True)
        self.build(root=archive, core=archive, available=False, default_target=True)

    def test_shallow_packed_refs_and_linked_worktree_select_exact_tree(self) -> None:
        original, _ = self.build()
        self.git("pack-refs", "--all", "--prune")
        self.assertEqual(self.build()[0], original)
        (self.root / "outside").write_text("outside-only commit\n")
        self.commit()
        shallow = self.base / "shallow"
        self.git("clone", "--quiet", "--depth=1", self.root.as_uri(), str(shallow))
        self.assertEqual(self.git("rev-parse", "--is-shallow-repository", root=shallow), "true")
        self.assertEqual(self.build(root=shallow)[0], original)
        linked = self.base / "linked"
        self.git("worktree", "add", "--quiet", "--detach", str(linked), "HEAD")
        self.assertTrue((linked / ".git").is_file())
        self.assertEqual(self.build(root=linked)[0], original)
        source = linked / CORE / "assets/input.txt"
        source.write_bytes(source.read_bytes() + b"linked source commit\n")
        self.commit(root=linked)
        self.assertNotEqual(self.build(root=linked)[0], original)
        self.assertEqual(self.build()[0], original)

    def test_sha256_format_wrong_owner_and_non_tree_revision_unavailable(self) -> None:
        self.build()
        other = self.base / "sha256"
        self.make_source(other)
        self.init(other, "sha256")
        self.build(root=other, available=False)
        owner = self.base / "wrong-owner"
        self.make_source(owner)
        self.init(owner)
        self.build(root=owner)
        moved = owner / "elsewhere/labcolors-core"
        moved.parent.mkdir()
        shutil.copytree(owner / CORE, moved, symlinks=True)
        (owner / "Cargo.toml").write_text('[workspace]\nmembers=["elsewhere/labcolors-core"]\nresolver="2"\n')
        self.build(root=owner, core=moved, available=False)
        blob = self.git("rev-parse", f"HEAD:{CORE}/assets/input.txt")
        self.git("rm", "--quiet", "--cached", "-r", str(CORE))
        self.git("update-index", "--add", "--cacheinfo", f"100644,{blob},{CORE}")
        self.git("commit", "--quiet", "--no-gpg-sign", "-m", "blob instead of selected tree")
        self.build(available=False)

    def test_tree_movement_during_observation_refuses(self) -> None:
        original, _ = self.build()
        a = self.git("rev-parse", "HEAD")
        source = self.root / CORE / "assets/input.txt"
        source.write_bytes(source.read_bytes() + b"other tree\n")
        b = self.commit()
        self.git("checkout", "--quiet", "--detach", a)
        tools = self.base / "git-observer"
        tools.mkdir()
        real_git = shutil.which("git", path=self.env["PATH"])
        self.assertIsNotNone(real_git)
        wrapper = tools / "git"
        # Делегирует каждый Git read настоящему Git; меняет только fixture HEAD
        # между двумя наблюдениями, без forged Git output или product test hook.
        wrapper.write_text(
            f"#!{sys.executable}\nimport os, subprocess, sys\n"
            f"real={real_git!r}\nroot={str(self.root)!r}\n"
            f"if 'ls-tree' in sys.argv: subprocess.run([real,'-C',root,'update-ref','HEAD',{b!r}],check=True)\n"
            "os.execv(real,[real,*sys.argv[1:]])\n"
        )
        wrapper.chmod(0o755)
        self.build(available=False, env={**self.env, "PATH": str(tools) + os.pathsep + self.env["PATH"]})
        self.git("checkout", "--quiet", "--detach", a)
        self.assertEqual(self.build()[0], original)


class HarnessLifetimeTests(unittest.TestCase):
    def test_all_cleanup_failures_remain_on_primary_timeout_traceback(self) -> None:
        primary = subprocess.TimeoutExpired(["PRIMARY_TIMEOUT"], timeout=1)
        process = mock.Mock(pid=999999999)
        process.communicate.side_effect = [primary, OSError("REAP")]
        process.kill.side_effect = OSError("DIRECT_STOP")
        with mock.patch("subprocess.Popen", return_value=process), \
                mock.patch("os.killpg", side_effect=OSError("GROUP_STOP")):
            with self.assertRaises(subprocess.TimeoutExpired) as captured:
                run_owned(["fixture"], cwd=Path.cwd(), env={}, timeout=1)
        self.assertIs(captured.exception, primary)
        rendered = "".join(traceback.format_exception(captured.exception))
        for witness in ("PRIMARY_TIMEOUT", "GROUP_STOP", "DIRECT_STOP", "REAP"):
            self.assertIn(witness, rendered)

    def test_secondary_cleanup_error_preserves_primary_exit(self) -> None:
        with mock.patch("os.killpg", side_effect=PermissionError("cleanup probe")):
            with self.assertRaises(subprocess.CalledProcessError) as captured:
                run_owned([sys.executable, "-c", "import sys; print('primary output'); sys.exit(7)"],
                          cwd=Path.cwd(), env=dict(os.environ), timeout=5)
        self.assertEqual(captured.exception.returncode, 7)
        self.assertEqual(captured.exception.stdout.strip(), "primary output")
        self.assertIn("secondary group stop failure", captured.exception.__notes__[0])

    @staticmethod
    def alive(pid: int) -> bool:
        try:
            # Zombie уже не исполняется; reap чужого orphan принадлежит OS init.
            return Path(f"/proc/{pid}/stat").read_text().split(") ", 1)[1][0] != "Z"
        except (FileNotFoundError, ProcessLookupError):
            return False

    def assert_stopped(self, pid: int) -> None:
        deadline = time.monotonic() + 2
        while self.alive(pid) and time.monotonic() < deadline:
            time.sleep(0.01)
        self.assertFalse(self.alive(pid), "owned descendant survived cleanup")

    @staticmethod
    def descendant_script(marker: Path, *, hang: bool, code: int = 0) -> str:
        return (
            "import pathlib,subprocess,sys,time,os\n"
            "child=subprocess.Popen([sys.executable,'-c','import time;time.sleep(60)'],"
            "stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)\n"
            f"pathlib.Path({str(marker)!r}).write_text(str(os.getpid())+' '+str(child.pid))\n"
            + ("time.sleep(60)\n" if hang else f"raise SystemExit({code})\n")
        )

    def test_success_failure_and_timeout_close_descendants_and_temp_directory(self) -> None:
        with tempfile.TemporaryDirectory(prefix="source-process-probe-") as control:
            for outcome in ("success", "failure", "timeout"):
                with self.subTest(outcome=outcome):
                    marker = Path(control) / outcome
                    fixture: str | None = None
                    try:
                        try:
                            with tempfile.TemporaryDirectory(dir=control) as fixture:
                                script = self.descendant_script(marker, hang=outcome == "timeout", code=7 if outcome == "failure" else 0)
                                result = run_owned([sys.executable, "-c", script], cwd=Path(fixture),
                                                   env=dict(os.environ), timeout=0.3 if outcome == "timeout" else 5)
                                self.assertNotEqual(outcome, "timeout", "timeout was swallowed")
                                self.assertEqual(result.returncode, 7 if outcome == "failure" else 0)
                        except subprocess.TimeoutExpired:
                            self.assertEqual(outcome, "timeout")
                        leader, child = map(int, marker.read_text().split())
                        self.assert_stopped(leader)
                        self.assert_stopped(child)
                        self.assertFalse(Path(fixture).exists())
                    finally:
                        # Cleanup самого hostile probe остаётся exact-owned и при RED.
                        if marker.exists():
                            leader = int(marker.read_text().split()[0])
                            try:
                                os.killpg(leader, signal.SIGKILL)
                            except ProcessLookupError:
                                pass

    def test_outer_sigterm_unwinds_active_group_and_temporary_directory(self) -> None:
        with tempfile.TemporaryDirectory(prefix="source-interruption-probe-") as control:
            control = Path(control)
            marker = control / "descendants"
            directory = control / "temporary-path"
            nested = (
                "import pathlib,runpy,sys,tempfile,os\n"
                f"owner=runpy.run_path({str(Path(__file__).resolve())!r})\n"
                "owner['install_interrupt_handlers']()\n"
                f"with tempfile.TemporaryDirectory(dir={str(control)!r}) as fixture:\n"
                f" pathlib.Path({str(directory)!r}).write_text(fixture)\n"
                f" owner['run_owned']([sys.executable,'-c',{self.descendant_script(marker, hang=True)!r}],"
                "cwd=pathlib.Path(fixture),env=dict(os.environ),timeout=30)\n"
            )
            owner = subprocess.Popen([sys.executable, "-c", nested], text=True,
                                     stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
            try:
                deadline = time.monotonic() + 5
                while not marker.exists() and time.monotonic() < deadline:
                    time.sleep(0.01)
                self.assertTrue(marker.exists(), "interruption probe never reached nested process")
                owner.send_signal(signal.SIGTERM)
                _, stderr = owner.communicate(timeout=5)
                self.assertNotEqual(owner.returncode, 0)
                self.assertIn("test harness interrupted by signal", stderr)
                self.assertFalse(Path(directory.read_text()).exists())
                for pid in map(int, marker.read_text().split()):
                    self.assert_stopped(pid)
            finally:
                for pid in [owner.pid, *([int(marker.read_text().split()[0])] if marker.exists() else [])]:
                    try:
                        os.killpg(pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                owner.communicate(timeout=5)


if __name__ == "__main__":
    install_interrupt_handlers()
    signal.alarm(210)
    try:
        unittest.main(verbosity=2, failfast=True)
    finally:
        signal.alarm(0)
