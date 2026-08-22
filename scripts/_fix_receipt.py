import json, hashlib, pathlib
p = pathlib.Path('crates/labcolors-core/contracts/clean-set-srgb8-v1/receipt-v1.json')
r = json.loads(p.read_text(encoding='utf-8'))
for i, a in enumerate(r['artifacts']):
    actual = pathlib.Path(a['path']).read_bytes()
    h = hashlib.sha256(actual).hexdigest()
    b = len(actual)
    if a.get('sha256') != h or a.get('bytes') != b:
        print(f'fix [{i}] {a["path"]}: sha={h} bytes={b}')
        a['sha256'] = h
        a['bytes'] = b
    keys = ['bytes', 'license', 'path', 'role', 'sha256']
    r['artifacts'][i] = {k: a[k] for k in keys if k in a}
out = json.dumps(r, indent=2, sort_keys=True, ensure_ascii=False) + '\n'
p.write_bytes(out.encode('utf-8'))
h2 = hashlib.sha256(out.encode('utf-8')).hexdigest()
sp = p.with_suffix('.sha256')
sp.write_bytes((h2 + '  receipt-v1.json\n').encode('ascii'))
print('receipt_sha:', h2)