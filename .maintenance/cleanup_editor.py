from pathlib import Path
import hashlib, json, re, subprocess

PARENT = '8af0264d024f8e799e0a06cc9c36801e85b0a880'
UPLOAD = 'cf430e030ddbb5b0abf93d22962f4752f3646cd9'
REMOVED = [
    'packages/colors/sink.js', 'packages/colors/test/sink.test.mjs',
    'docs/r7-tech-debt-audit.md', 'docs/r8-wave1-status.md',
    'epics/lab-colors-c20-o13-wave/EPIC.md',
    'epics/lab-colors-c20-o13-wave/RATIFY.md',
    'epics/lab-colors-c20-o13-wave/STATE.json',
]
TEST_PATH = 'scripts/test_documentation_integrity.py'
TEST_SOURCE = '''"""Читаемость отслеживаемой Markdown-документации."""
from pathlib import Path
import subprocess
import unittest


def validate_markdown(data: bytes) -> None:
    data.decode("utf-8", errors="strict")
    for value in data:
        if value < 32 and value not in (9, 10, 13):
            raise ValueError(f"управляющий байт 0x{value:02x}")


class DocumentationIntegrityTests(unittest.TestCase):
    def test_readable_unicode_and_whitespace_are_preserved(self):
        validate_markdown("# Цвет\\n\\nТекст\\tи код.\\r\\n".encode())

    def test_control_bytes_are_rejected(self):
        for value in set(range(32)) - {9, 10, 13}:
            with self.subTest(value=value), self.assertRaises(ValueError):
                validate_markdown(b"before" + bytes([value]) + b"after")

    def test_invalid_utf8_is_rejected(self):
        with self.assertRaises(UnicodeDecodeError):
            validate_markdown(b"# title\\n\\xff")

    def test_tracked_markdown_is_readable(self):
        root = Path(__file__).resolve().parent.parent
        result = subprocess.run(
            ["git", "ls-files", "-z", "--", "*.md"], cwd=root,
            check=True, stdout=subprocess.PIPE,
        )
        paths = result.stdout.decode("utf-8", errors="strict").split("\\0")
        self.assertTrue(any(paths))
        for name in filter(None, paths):
            with self.subTest(path=name):
                validate_markdown((root / name).read_bytes())


if __name__ == "__main__":
    unittest.main()
'''

def seal(value):
    value.pop('record_sha256', None)
    raw = json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()
    value['record_sha256'] = hashlib.sha256(raw).hexdigest()
    return json.dumps(value, sort_keys=True, indent=1, ensure_ascii=False) + '\n'


def blob(content):
    data = content.encode()
    return hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()


def repair_comment(line):
    if not line.lstrip().startswith('//'):
        return line
    def decode(match):
        value = match.group()
        for _ in range(2):
            try:
                data = b''.join(bytes([ord(ch)]) if 128 <= ord(ch) <= 159 else ch.encode('cp1251') for ch in value)
                corrected = data.decode('utf-8')
            except (UnicodeEncodeError, UnicodeDecodeError):
                break
            if corrected == value:
                break
            value = corrected
        return value
    return re.sub(r'[^\x00-\x7f]+', decode, line)


def prepare(read, paths):
    changes = {}
    removals = {}
    for path in REMOVED:
        content = read(path)
        removals[path] = blob(content)
        changes[path] = None
    changed_comments = {}
    for path in ['crates/labcolors-core/src/program_wire.rs', 'crates/labcolors-core/src/program/wire.rs']:
        before = read(path)
        lines = before.splitlines(keepends=True)
        after = ''.join(repair_comment(line) for line in lines)
        after = after.replace(
            '/// Единственная версия, которую понимает этот декодер. Field-секции (C7e\n/// stage 2) войдут НОВОЙ версией, а не форком формата.',
            '/// Базовая версия wire без авторского порядка конечных состояний.\n/// Общее декодирование дополнительно принимает расширение v2.',
        ).replace(
            '    /// Детализация класса намеренно не публикуется в v1: полный typed\n    /// compile-контракт публикует атомарный C7c; преждевременная строковая\n    /// проекция 62 внутренних классов стала бы Hyrum-контрактом до него.',
            '    /// Публичная категория сохраняется независимо от внутренних причин отказа.\n    /// Детали компиляции остаются у компилятора; этот вход возвращает\n    /// только результат проверки графа.',
        ).replace(
            '/// Family-графы в первом публичном runtime-срезе отклоняются: доверие к family\n/// artifact обеспечивает вызывающий, а public trust-параметр будет отдельной\n/// версией seam, не silent assumption.',
            '/// Графы, которым нужен внешний family-артефакт, получают явный отказ.\n/// Этот вход не принимает сведения о доверии к артефакту и не создаёт\n/// такое доверие по умолчанию.',
        )
        assert [line for line in before.splitlines() if not line.lstrip().startswith('//')] == [line for line in after.splitlines() if not line.lstrip().startswith('//')]
        changed_comments[path] = sum(a != b for a, b in zip(lines, after.splitlines(keepends=True)))
        if after != before:
            changes[path] = after
    path = 'proof/region/v1/PROTOCOL.md'
    before = read(path)
    if before.count('\0') != 1:
        raise RuntimeError('Unexpected protocol control-byte count')
    changes[path] = before.replace('\0', r'\0')
    upload_sites = {}
    for path in paths:
        if not path.startswith('.github/workflows/') or not path.endswith(('.yml', '.yaml')):
            continue
        before = read(path)
        after, count = re.subn(
            r'(?m)^(\s*uses: actions/upload-artifact@)[0-9a-f]{40}(?:\s*# v4[^\n]*)?$',
            r'\g<1>' + UPLOAD + ' # v7.0.2', before,
        )
        if count:
            upload_sites[path] = count
            changes[path] = after
    if sum(upload_sites.values()) != 15:
        raise RuntimeError(('Upload sites changed', upload_sites))
    changes[TEST_PATH] = TEST_SOURCE
    tests = json.loads(read('proof/artifact/tests.json'))
    suite = next(suite for suite in tests['python'] if suite['start'] == 'scripts')
    for name in [
        'test_readable_unicode_and_whitespace_are_preserved',
        'test_control_bytes_are_rejected', 'test_invalid_utf8_is_rejected',
        'test_tracked_markdown_is_readable',
    ]:
        name = 'test_documentation_integrity.DocumentationIntegrityTests.' + name
        if name in suite['names']:
            raise RuntimeError('Test already exists')
        suite['names'].append(name)
    suite['names'].sort()
    suite['count'] = len(suite['names'])
    changes['proof/artifact/tests.json'] = seal(tests)
    record = json.loads(read('proof/artifact/tree.json'))
    ignored_parts = {'node_modules','target','pkg','__pycache__','.build'}
    def included(path):
        return not path.startswith('proof/artifact/') and not any(part in ignored_parts for part in path.split('/')) and any(path == root or path.startswith(root + '/') for root in record['roots'])
    for path, content in changes.items():
        if not included(path):
            continue
        if content is None:
            record['files'].pop(path, None)
        else:
            record['files'][path] = blob(content)
    record['files'] = dict(sorted(record['files'].items()))
    record['file_count'] = len(record['files'])
    changes['proof/artifact/tree.json'] = seal(record)
    return changes, {'removed':removals, 'comment_lines':changed_comments, 'upload_sites':upload_sites}


def main():
    root = Path.cwd()
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    if head != PARENT:
        raise RuntimeError('Unexpected source parent')
    paths = subprocess.check_output(['git','ls-files','-z'], text=True).split('\0')
    changes, report = prepare(lambda path: (root/path).read_text(encoding='utf-8'), paths)
    repository = 'Labpics-Team/lab-colors'
    def api(path, value=None):
        command = ['gh','api',path]
        if value is not None:
            command += ['--method','POST','--input','-']
        result = subprocess.run(command, input=json.dumps(value) if value is not None else None, text=True, capture_output=True, check=True)
        return json.loads(result.stdout)
    parent = api(f'repos/{repository}/git/commits/{PARENT}')
    tree = []
    for path, content in sorted(changes.items()):
        entry = {'path':path,'type':'blob','mode':'100644'}
        entry['sha' if content is None else 'content'] = content
        tree.append(entry)
    prepared = api(f'repos/{repository}/git/trees', {'base_tree':parent['tree']['sha'],'tree':tree})
    candidate = api(f'repos/{repository}/git/commits', {
        'message':'chore(colors): retire obsolete source paths and preserve readable documentation',
        'tree':prepared['sha'], 'parents':[PARENT],
    })
    report.update(parent=PARENT, commit=candidate['sha'], tree=prepared['sha'], changes=changes)
    output = Path('/tmp/colors-source-preparation')
    output.mkdir(exist_ok=True)
    (output/'cleanup.json').write_text(json.dumps(report,ensure_ascii=False,indent=2))
    print(json.dumps({key:value for key,value in report.items() if key!='changes'}, ensure_ascii=False))

if __name__=='__main__':
    main()
