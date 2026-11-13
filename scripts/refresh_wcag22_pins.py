#!/usr/bin/env python3
"""Refresh all pins after a change to wcag22_evidence.rs / q55-proof.

Steps:
1. Regenerate conformance vectors: cargo run -p labcolors-conformance --bin gen
2. Recompute sha256 of the canonical sources.
3. Update pins in verify_wcag22_q55.py, verify_point_support_surplus.py,
   wcag22.rs and pack_contract.rs, preserving each file's formatting.
4. Run both verifiers to confirm PASS.

IMPORTANT: --emit writes files with CRLF on Windows; normalize to LF before hashing.
Never edit pins by hand -- always regenerate from the canonical source.
"""

import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]

Q55_PROOF = REPO / 'crates' / 'labcolors-core' / 'contracts' / 'wcag22-srgb8-q55-proof-v1.json'
EVIDENCE_RS = REPO / 'crates' / 'labcolors-core' / 'src' / 'wcag22_evidence.rs'
WCAG22_RS = REPO / 'crates' / 'labcolors-core' / 'src' / 'wcag22.rs'
Q55_SCRIPT = REPO / 'scripts' / 'verify_wcag22_q55.py'
SURPLUS_SCRIPT = REPO / 'scripts' / 'verify_point_support_surplus.py'
PACK_CONTRACT = REPO / 'crates' / 'labcolors-conformance' / 'tests' / 'pack_contract.rs'


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def sha256_norm(path: Path) -> str:
    """Hash after normalizing CRLF -> LF (Windows --emit writes CRLF)."""
    return hashlib.sha256(path.read_bytes().replace(b'\r\n', b'\n')).hexdigest()


def replace_pin(path: Path, pattern: str, replacement: str) -> None:
    """Replace exactly one regex match; fail loudly otherwise."""
    s = path.read_text(encoding='utf-8')
    new_s, count = re.subn(pattern, replacement, s)
    assert count == 1, f'{path}: expected exactly one match for {pattern!r}, got {count}'
    path.write_text(new_s, encoding='utf-8', newline='\n')


def run(cmd) -> None:
    print('$', ' '.join(cmd))
    subprocess.run(cmd, check=True, cwd=REPO)


def main() -> None:
    # 1. Regenerate conformance vectors from canonical kernel
    run(['cargo', 'run', '-p', 'labcolors-conformance', '--bin', 'gen', '--locked'])

    # 2. Update pins (formats below mirror each file exactly)
    evidence_sha = sha256_norm(EVIDENCE_RS)
    proof_sha = sha256_norm(Q55_PROOF)
    payload_sha = json.loads(Q55_PROOF.read_text(encoding='utf-8-sig'))['proof_payload_sha256']
    verifier_sha = sha256(Q55_SCRIPT)

    replace_pin(
        Q55_SCRIPT,
        r'EXPECTED_TERMINAL_EVIDENCE_SHA256 = \(\n    "[0-9a-f]{64}"\n\)',
        f'EXPECTED_TERMINAL_EVIDENCE_SHA256 = (\n    "{evidence_sha}"\n)',
    )
    replace_pin(
        WCAG22_RS,
        r'const PROOF_SOURCE_SHA256: &str =\n    "[0-9a-f]{64}";',
        f'const PROOF_SOURCE_SHA256: &str =\n    "{proof_sha}";',
    )
    replace_pin(
        WCAG22_RS,
        r'const PROOF_PAYLOAD_SHA256: &str =\n    "[0-9a-f]{64}";',
        f'const PROOF_PAYLOAD_SHA256: &str =\n    "{payload_sha}";',
    )
    replace_pin(
        WCAG22_RS,
        r'const VERIFIER_SHA256: &str = "[0-9a-f]{64}";',
        f'const VERIFIER_SHA256: &str = "{verifier_sha}";',
    )
    replace_pin(
        SURPLUS_SCRIPT,
        r'EXPECTED_Q55_PROOF_SHA256 = \(\n    "[0-9a-f]{64}"\n\)',
        f'EXPECTED_Q55_PROOF_SHA256 = (\n    "{proof_sha}"\n)',
    )
    replace_pin(
        SURPLUS_SCRIPT,
        r'EXPECTED_Q55_PAYLOAD_SHA256 = \(\n    "[0-9a-f]{64}"\n\)',
        f'EXPECTED_Q55_PAYLOAD_SHA256 = (\n    "{payload_sha}"\n)',
    )
    replace_pin(
        PACK_CONTRACT,
        r'("wcag22\.json",\n        ")[0-9a-f]{64}(")',
        rf'\g<1>{sha256(REPO / "conformance" / "vectors" / "wcag22.json")}\g<2>',
    )

    # 3. Verify
    python = 'python' if sys.platform == 'win32' else 'python3'
    run([python, 'scripts/verify_wcag22_q55.py'])
    run([python, 'scripts/verify_point_support_surplus.py'])
    print('All pins refreshed and verified.')


if __name__ == '__main__':
    main()