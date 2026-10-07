#!/usr/bin/env python3
"""Independent exhaustive constraint search directly from source XML and ZIP."""
import collections
import json
from pathlib import Path
import re
import sys
from xml.etree import ElementTree as ET
import zipfile

ROOT = Path(__file__).resolve().parent
NAMES = set('red green yellow blue orange pink purple brown black gray white'.split())


def require(value, label):
    if not value:
        raise ValueError(label)


article = ET.parse(ROOT / 'input/article.xml').getroot()
table, = article.findall(".//table-wrap[@id='table-3']//tbody")
published = {}
for tr in table.findall('tr'):
    cells = [''.join(td.itertext()).strip().lower() for td in tr.findall('td')]
    require(len(cells) == 3, 'three Table3 cells')
    name = cells[0].replace('*', '')
    require(name not in published, 'unique source label')
    parsed = []
    for cell in cells[1:]:
        pairs = re.findall(r'([a-z]+)\((\d+)\)', cell)
        require(pairs and len(dict(pairs)) == len(pairs), 'unique published names')
        parsed.append({color: int(percent) for color, percent in pairs})
    published[name] = {'normal': parsed[0], 'cvd': parsed[1]}
require(len(published) == 32, '32 named source rows')

counts = {group: [collections.Counter() for _ in range(32)] for group in ['cvd', 'normal']}
roster = set()
with zipfile.ZipFile(ROOT / 'input/original-sato-data.zip') as archive:
    require(len(archive.namelist()) == len(set(archive.namelist())), 'unique ZIP entries')
    for path in archive.namelist():
        if not path.startswith('dataset_ver2/naming/') or path.endswith('/'):
            continue
        match = re.fullmatch(r'dataset_ver2/naming/(CVD|NTF|NTM)/\1subject(\d{3})_([12])_colornaming\.txt', path)
        require(match, 'canonical naming filename')
        cohort, subject, session = match.groups()
        identity = (cohort, int(subject), int(session))
        require(identity not in roster, 'unique participant session')
        roster.add(identity)
        rows = archive.read(path).decode('ascii').splitlines()
        require(len(rows) == 32, '32 physical naming records')
        seen = set()
        for line in rows:
            fields = line.split('\t')
            require(len(fields) == 5 and all(re.fullmatch(r'\d+', field) for field in fields), 'five integer naming columns')
            stimulus, _, _, code, _ = map(int, fields)
            require(0 <= stimulus < 32 and stimulus not in seen and 0 <= code <= 10, 'unique stimulus/code domains')
            seen.add(stimulus)
            counts['cvd' if cohort == 'CVD' else 'normal'][stimulus][code] += 1
        require(seen == set(range(32)), 'complete naming stimulus set')
require(roster == {(cohort, person, session) for cohort, size in [('CVD', 13), ('NTM', 13), ('NTF', 10)] for person in range(1, size + 1) for session in (1, 2)}, 'exact 72 participant-session roster')
for group, denominator in [('cvd', 26), ('normal', 46)]:
    require(all(sum(row.values()) == denominator for row in counts[group]), 'group denominator')


def row_options(row, groups, half_percent_units):
    """All compatible local injections; unlisted names must be under 15%."""
    names = set().union(*(set(row[group]) for group in groups))
    result = []
    for stimulus in range(32):
        allowed = {}
        for name in sorted(names):
            allowed[name] = []
            for code in range(11):
                good = True
                for group in groups:
                    denominator = 26 if group == 'cvd' else 46
                    count = counts[group][stimulus][code]
                    if name in row[group]:
                        good &= abs(200 * count - 2 * row[group][name] * denominator) <= half_percent_units * denominator
                    else:
                        good &= 100 * count < 15 * denominator
                if good:
                    allowed[name].append(code)
        def inject(remaining, mapping, taken):
            if not remaining:
                for code in set(range(11)) - taken:
                    if any(100 * counts[group][stimulus][code] >= 15 * (26 if group == 'cvd' else 46) for group in groups):
                        return
                result.append((stimulus, mapping.copy()))
                return
            name = min(remaining, key=lambda item: (len(set(allowed[item]) - taken), item))
            for code in allowed[name]:
                if code not in taken:
                    inject(remaining - {name}, mapping | {name: code}, taken | {code})
        inject(names, {}, set())
    return result


def solve(half_percent_units, anchor):
    options = {label: row_options(row, ['cvd', 'normal'] if anchor == 'all' or label == anchor else ['cvd'], half_percent_units) for label, row in published.items()}
    solutions = []
    visits = 0
    def dfs(remaining, assignments, code_map, used):
        nonlocal visits
        visits += 1
        if not remaining:
            # Complete the color/code bijection; do not assume button/code order.
            missing = NAMES - set(code_map)
            spare = set(range(11)) - set(code_map.values())
            require(len(missing) == len(spare), 'complete bijection domains')
            def complete(names, available, mapping):
                if not names:
                    solutions.append({'labels_to_raw_ids': assignments.copy(), 'names_to_codes': mapping})
                else:
                    name = min(names)
                    for code in available:
                        complete(names - {name}, available - {code}, mapping | {name: code})
            complete(missing, spare, code_map)
            return
        viable = {}
        for label in sorted(remaining):
            choices = []
            for stimulus, local in options[label]:
                if stimulus in used:
                    continue
                combined = code_map | local
                if any(name in code_map and code_map[name] != code for name, code in local.items()):
                    continue
                if len(set(combined.values())) != len(combined):
                    continue
                choices.append((stimulus, combined))
            if not choices:
                return
            viable[label] = choices
        label = min(remaining, key=lambda item: (len(viable[item]), item))
        for stimulus, combined in viable[label]:
            dfs(remaining - {label}, assignments | {label: stimulus}, combined, used | {stimulus})
    dfs(set(published), {}, {}, set())
    return {'half_percent_units': half_percent_units, 'normal_constraint': anchor, 'solution_count': len(solutions), 'search_states': visits, 'solutions': solutions}


saved = json.loads((ROOT / 'input/STIMULUS-JOIN.json').read_text())
expected = {row['label']: row['raw_stimulus_id'] for row in saved['rows']}
require(len(expected) == 32 and set(expected.values()) == set(range(32)), 'saved candidate bijection')
reports = []
for enclosure in (1, 2):
    for anchor in ('saturated green', 'muted green'):
        report = solve(enclosure, anchor)
        require(report['solution_count'] == 1, 'unique conditional joint mapping')
        require(report['solutions'][0]['labels_to_raw_ids'] == expected, 'independent mapping equals saved candidate')
        reports.append(report)
    full = solve(enclosure, 'all')
    require(full['solution_count'] == 0, 'published source conflict must remain visible')
    reports.append(full)

result = {'verdict': 'PASS_CONDITIONAL', 'claim': 'Exhaustive independent joint code-name and raw-id/label search confirms the saved mapping under all CVD constraints plus either normal green anchor, for both declared percent enclosures. Full Table3 is inconsistent.', 'source_naming_records': 2304, 'source_named_rows': 32, 'roster_sessions': len(roster), 'reports': reports, 'non_claims': ['Author-provided stimulus codebook', 'Resolution of full Table3 inconsistency', 'Independent experimental validation', 'Historical ac6 equivalence', 'Physical or human admission'], 'premises': ['First naming token is stimulus id; fourth is categorical code', 'Two sessions pooled; normals pooled NTF+NTM', 'Omitted published response frequencies are below 15%', 'Printed percentages lie within ±0.5 or ±1 percentage point', 'This explicitly conditional association is retained instead of repairing published cells']}
(ROOT / sys.argv[1] / 'result.json').write_text(json.dumps(result, indent=2, sort_keys=True) + '\n')
print(json.dumps({key: value for key, value in result.items() if key != 'reports'}))
print(json.dumps([{key: value for key, value in report.items() if key != 'solutions'} for report in reports]))
