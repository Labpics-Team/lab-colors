#!/usr/bin/env python3
"""Offline replay of the public, explicitly conditional nominal source model."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check_inputs():
    manifest = json.loads((ROOT / 'PROOF-MANIFEST.json').read_text())
    names = [item['path'] for item in manifest['files']]
    require(len(names) == len(set(names)), 'unique proof manifest paths')
    for item in manifest['files']:
        path = ROOT / item['path']
        require(not Path(item['path']).is_absolute() and '..' not in Path(item['path']).parts,
                'relative proof path')
        require(path.resolve().is_relative_to(ROOT) and
                not any((ROOT / Path(*Path(item['path']).parts[:index])).is_symlink()
                        for index in range(1, len(Path(item['path']).parts) + 1)),
                'proof inputs must not traverse symbolic links')
        require(path.is_file() and not path.is_symlink(), 'regular proof input')
        require(path.stat().st_size == item['bytes'] and digest(path) == item['sha256'],
                'proof input identity: ' + item['path'])
    return manifest


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', '--output-dir', required=True, type=Path, help='fresh result directory')
    parser.add_argument('--mode', choices=('normal', 'optimized', 'both'))
    parser.add_argument('--product-root', type=Path,
                        help='also bind current product Rust source bytes; required by the product full gate')
    args = parser.parse_args()
    output = args.output.resolve()
    require(not output.exists(), 'result directory must be fresh')
    manifest = check_inputs()
    bridge = json.loads((ROOT / 'geometry/input/bridge.json').read_text())
    if args.product_root is not None:
        for binding in bridge['bindings']:
            require(digest(args.product_root / binding['path']) == binding['sha256'],
                    'current product bridge source: ' + binding['path'])
    # Bind duplicate audit inputs explicitly; neither implementation may receive
    # a different source while still emitting a plausible independent PASS.
    shared = [
        ('audits/bridge/input/bridge.json', 'geometry/input/bridge.json'),
        ('audits/join/input/STIMULUS-JOIN.json', 'geometry/input/join.json'),
        ('audits/join/input/article.xml', 'primary/colors-sato-input-identity-20261002/primary/article.xml'),
        ('audits/join/input/original-sato-data.zip', 'primary/colors-sato-input-identity-20261002/primary/original-sato-data.zip'),
    ]
    for left, right in shared:
        require((ROOT / left).read_bytes() == (ROOT / right).read_bytes(), 'cross-audit source equality')
    output.mkdir(parents=True)
    selected_mode = args.mode or ('optimized' if not __debug__ else 'both')
    require(__debug__ or selected_mode != 'normal', 'optimized launcher cannot request an unoptimized replay')
    modes = ('normal', 'optimized') if selected_mode == 'both' else (selected_mode,)
    reports = {}
    env = dict(os.environ, PYTHONDONTWRITEBYTECODE='1')
    for mode in modes:
        interpreter = [sys.executable, '-I', '-B'] + (['-O'] if mode == 'optimized' else [])
        dest = output / mode
        dest.mkdir()
        for stage in ('primary', 'bridge', 'join', 'geometry', 'delta'):
            (dest / stage).mkdir()
        commands = [
            ('primary', [str(ROOT / 'audit_primary_inputs.py'), '--archive', str(ROOT / 'primary'),
                         '--capsule', str(ROOT / 'geometry'), '--output', str(dest / 'primary/result.json')]),
            ('bridge', [str(ROOT / 'audits/bridge/audit_bridge.py'), str(dest / 'bridge')]),
            ('join', [str(ROOT / 'audits/join/audit_join.py'), str(dest / 'join')]),
            ('geometry', [str(ROOT / 'geometry/verify_exact_v2.py'), str(dest / 'geometry')]),
            ('delta', [str(ROOT / 'compare_finite_tables.py'), str(ROOT / 'historical-v1/old-column-rle.bin'),
                       str(dest / 'geometry/derived.raw'), str(dest / 'delta')]),
        ]
        for stage, argv in commands:
            started = time.monotonic()
            with (dest / stage / 'stdout.txt').open('wb') as stdout, (dest / stage / 'stderr.txt').open('wb') as stderr:
                result = subprocess.run(interpreter + argv, cwd=ROOT, env=env, stdout=stdout, stderr=stderr, timeout=600)
            require(result.returncode == 0, mode + '/' + stage + ' failed; see captured stderr')
            print(json.dumps({'mode':mode, 'stage':stage, 'status':'PASS',
                              'seconds':round(time.monotonic()-started, 3)}), flush=True)
        require((dest / 'delta/new-column-rle.bin').read_bytes() == (ROOT / 'derived/new-column-rle.bin').read_bytes(),
                'canonical codec output identity')
        require(json.loads((dest / 'delta/delta.json').read_text()) ==
                json.loads((ROOT / 'historical-v1/DELTA.json').read_text()),
                'historical output delta must reproduce its preserved record')
        reports[mode] = {stage: json.loads((dest / stage / ('delta.json' if stage == 'delta' else 'result.json')).read_text())
                         for stage in ('primary', 'bridge', 'join', 'geometry', 'delta')}
    if len(modes) == 2:
        comparable = json.loads(json.dumps(reports))
        for mode in modes:
            comparable[mode]['geometry'].pop('runtime_seconds', None)
        require(comparable['normal'] == comparable['optimized'], 'normal/optimized report equality')
        require((output / 'normal/geometry/derived.raw').read_bytes() ==
                (output / 'optimized/geometry/derived.raw').read_bytes(), 'normal/optimized raw equality')
    check_inputs()
    chosen = output / modes[0]
    shutil.copyfile(chosen / 'geometry/derived.raw', output / 'derived.raw')
    shutil.copyfile(chosen / 'delta/new-column-rle.bin', output / 'derived-codec.bin')
    report = {
        'status':'PASS_CONDITIONAL_NOMINAL',
        'proof_manifest_sha256':digest(ROOT / 'PROOF-MANIFEST.json'),
        'modes':list(modes), 'current_product_source_checked':args.product_root is not None,
        'source_file_count':len(manifest['files']),
        'raw_sha256':reports[modes[0]]['geometry']['table_sha256'],
        'table_sha256':reports[modes[0]]['geometry']['table_sha256'],
        'codec_sha256':reports[modes[0]]['delta']['new_codec_sha256'],
        'accepted_points':reports[modes[0]]['geometry']['cube_outcomes']['accepted'],
        'domain_points':reports[modes[0]]['geometry']['cube_points'],
        'cube_outcomes':reports[modes[0]]['geometry']['cube_outcomes'],
        'delta':reports[modes[0]]['delta']['counts'],
        'stages':{mode:{stage:{'status':'PASS', 'result_sha256':digest(output / mode / stage / ('delta.json' if stage == 'delta' else 'result.json'))}
                        for stage in ('primary','bridge','join','geometry','delta')} for mode in modes},
        'limits':manifest['limits'],
    }
    (output / 'result.json').write_text(json.dumps(report,indent=2,ensure_ascii=False)+'\n')
    print(json.dumps(report,ensure_ascii=False),flush=True)


if __name__ == '__main__':
    main()
