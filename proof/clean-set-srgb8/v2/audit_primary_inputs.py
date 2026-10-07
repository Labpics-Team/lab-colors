#!/usr/bin/env python3
"""Independent primary-byte audit; no imports from upstream derivation code.

Exact aggregation uses integer hundredths and reduces only final rational means.
The join's *source identity* is conditional; this checks Table S1 tokens under
the declared join and does not turn that join into an author-supplied codebook.
"""
import argparse
import hashlib
import json
import re
import zipfile
from fractions import Fraction
from pathlib import Path
from xml.etree import ElementTree as ET


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read_json(path):
    return json.loads(path.read_text())


def sha(data):
    return hashlib.sha256(data).hexdigest()


def parse_responses(data):
    lines = data.decode('ascii').splitlines()
    removed_final_blank = bool(lines and not lines[-1])
    if removed_final_blank:
        lines.pop()
    require(len(lines) == 32, 'response row count')
    responses = {}
    for line in lines:
        match = re.fullmatch(r'([0-9]{2})\t([0-9]{2})\.([0-9]{2})\t([0-9]+)', line)
        require(match is not None, 'response lexical structure')
        sid, integer, decimals, _latency = match.groups()
        sid = int(sid)
        require(sid not in responses, 'duplicate stimulus id')
        value = int(integer) * 100 + int(decimals)
        require(0 <= value <= 1000, 'rating domain')
        responses[sid] = value
    require(set(responses) == set(range(32)), 'stimulus census')
    return responses, removed_final_blank


def aggregate(records):
    """records maps (cohort,participant,session) to all32 integer responses."""
    require(len(records) == 72, 'session file count')
    rows = []
    for cohort, participants in [('NTF', 10), ('NTM', 13), ('CVD', 13)]:
        required = {(cohort, participant, session)
                    for participant in range(1, participants + 1)
                    for session in (1, 2)}
        require({key for key in records if key[0] == cohort} == required,
                'participant/session census')
        for sid in range(32):
            total = sum(records[key][sid] for key in required)
            mean = Fraction(total, participants * 2 * 100)
            diff = mean - 5
            rows.append({
                'CohortId': cohort, 'StimulusId': sid,
                'participant_count': participants, 'session_count_each': 2,
                'mean': {'numerator': mean.numerator, 'denominator': mean.denominator},
                'mean_minus_midpoint': {'numerator': diff.numerator, 'denominator': diff.denominator},
                'relation_to_5': '>' if diff > 0 else '<' if diff < 0 else '=',
                'non_clean_canonical_ge_5': mean >= 5,
                'non_clean_sensitivity_gt_5': mean > 5,
            })
    require({key[0] for key in records} == {'NTF', 'NTM', 'CVD'}, 'cohort census')
    return rows


def reject_control(name, thunk):
    try:
        thunk()
    except (ValueError, KeyError):
        return name
    raise ValueError('negative control admitted: ' + name)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--archive', required=True, type=Path)
    parser.add_argument('--capsule', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    archive, capsule = args.archive, args.capsule
    acquisition = read_json(archive / 'ACQUISITION.json')
    require(acquisition['commit'] == 'c6b32fb2378346098960bb776b9ca31e4f328c39', 'archive commit')
    require(len(acquisition['files']) == 9, 'archive file census')
    for item in acquisition['files']:
        data = (archive / item['path']).read_bytes()
        require(sha(data) == item['sha256'] and len(data) == item['bytes'], 'archive sha256/size')
        require(hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
                == item['git_blob'], 'archive git blob')
    identity = archive / 'colors-sato-input-identity-20261002'
    zip_path = identity / 'primary/original-sato-data.zip'
    require(sha(zip_path.read_bytes()) == '22cdecae96e9fd2b1af1f09a4c4358cbacefb12cee82d7fa5374fcfe2ed8edad', 'Sato ZIP identity')
    records, final_blanks = {}, []
    with zipfile.ZipFile(zip_path) as z:
        names = [name for name in z.namelist() if name.startswith('dataset_ver2/affective/cleanliness/') and not name.endswith('/')]
        require(len(names) == len(set(names)) == 72, 'unique ZIP cleanliness member census')
        for name in names:
            match = re.fullmatch(r'dataset_ver2/affective/cleanliness/(CVD|NTF|NTM)/\1subject([0-9]{3})_([12])_cleanness.txt', name)
            require(match is not None, 'filename identity')
            cohort, participant, session = match.groups()
            key = (cohort, int(participant), int(session))
            require(key not in records, 'duplicate session')
            records[key], blank = parse_responses(z.read(name))
            if blank:
                final_blanks.append(name)
        sample = z.read(names[0])
    means = aggregate(records)
    expected = read_json(archive / 'colors-sato-exact-witnesses-20261002/results/COHORT-MEANS.json')
    require(means == expected['rows'], 'exact96 source cohort means')
    witnesses = [row for row in means if row['non_clean_canonical_ge_5']]
    capsule_witnesses = read_json(capsule / 'input/witnesses.json')
    require(len(witnesses) == 48 and witnesses == capsule_witnesses['rows'], 'exact48 capsule witness rows')
    require(capsule_witnesses['source_zip_sha256'] == sha(zip_path.read_bytes()), 'witness source identity')

    docx_path = identity / 'primary/TableS1.docx'
    join = read_json(capsule / 'input/join.json')
    require(join == read_json(identity / 'STIMULUS-JOIN.json'), 'join archive/capsule binding')
    require(join['table_s1_docx_sha256'] == sha(docx_path.read_bytes()), 'join TableS1 identity')
    with zipfile.ZipFile(docx_path) as z:
        document = ET.fromstring(z.read('word/document.xml'))
    ns = {'w': 'http://schemas.openxmlformats.org/wordprocessingml/2006/main'}
    tables = document.findall('.//w:tbl', ns)
    require(len(tables) == 1, 'TableS1 table census')
    table_rows = [[ ''.join(t.text or '' for t in cell.findall('.//w:t', ns))
                    for cell in row.findall('w:tc', ns)]
                  for row in tables[0].findall('w:tr', ns)]
    require(len(table_rows) == 34 and len(join['rows']) == 32, 'TableS1 row census')
    require({row['raw_stimulus_id'] for row in join['rows']} == set(range(32)), 'join raw id census')
    require({row['table_s1_row_index'] for row in join['rows']} == set(range(2, 34)), 'join TableS1 bijection')
    for row in join['rows']:
        actual = table_rows[row['table_s1_row_index']]
        require(actual[0] == row['label'], 'join stimulus label')
        require(actual[2:] == [row['xyY_decimal_tokens'][key] for key in ('x','y','Y')], 'TableS1 exact decimal tokens')

    article = ET.parse(identity / 'primary/article.xml').getroot()
    doi = [el.text for el in article.findall('.//article-id') if el.attrib.get('pub-id-type') == 'doi']
    require(doi == ['10.7717/peerj.2751'], 'article DOI')
    license_uris = [el.attrib.get('{http://www.w3.org/1999/xlink}href') for el in article.findall('.//permissions/license')]
    require('http://creativecommons.org/licenses/by/4.0/' in license_uris, 'article source CC BY4')
    cie = []
    for name, doi, length in [('cmf','10.25039/CIE.DS.xvudnb9b',471), ('d65','10.25039/CIE.DS.hjfjmt59',531)]:
        primary = archive / 'colors-nominal-model-input-closure-20261002/primary'
        metadata = read_json(primary / ('cie-' + name + '-metadata.json'))
        data = (primary / ('cie-' + name + '.csv')).read_bytes()
        require(data == (capsule / ('input/cie-' + name + '.csv')).read_bytes(), 'CIE capsule byte binding')
        require(metadata['identifier']['identifier'] == doi, 'CIE DOI')
        for checksum in metadata['checksums']:
            require(hashlib.new(checksum['hashMethod'], data).hexdigest() == checksum['checksum'], 'CIE published checksum')
        require(any(x['rightsURI'] == 'https://creativecommons.org/licenses/by-sa/4.0/' for x in metadata['rightsList']), 'CIE CC BY-SA4')
        lines = data.decode().splitlines()
        require(len(lines) == length, 'CIE native row count')
        wavelengths = [int(line.split(',')[0]) for line in lines]
        require(wavelengths == list(range(360 if name == 'cmf' else 300, 831)), 'CIE native wavelength census')
        require(len([x for x in wavelengths if 360 <= x <= 780]) == 421, 'CIE nominal support421')
        cie.append({'name':name,'doi':doi,'sha256':sha(data),'native_rows':length,'nominal_rows':421,'license':'CC-BY-SA-4.0'})

    # Exercise lexical/census rejection gates, preserving an unmodified positive
    # parse and aggregation before mutations. These are targeted tests, not a
    # claim to mutate/replay the whole geometry for each malformed raw file.
    sample_lines = sample.decode('ascii').splitlines()
    if not sample_lines[-1]:
        sample_lines.pop()
    require(parse_responses(('\n'.join(sample_lines)).encode())[0] == parse_responses(sample)[0], 'positive line ending control')
    bad_duplicate = list(sample_lines)
    bad_duplicate[1] = bad_duplicate[0]
    bad_rating = list(sample_lines)
    cells = bad_rating[0].split('\t')
    cells[1] = '10.01'
    bad_rating[0] = '\t'.join(cells)
    missing = dict(records)
    missing.pop(next(iter(missing)))
    controls = [
        reject_control('missing-response', lambda: parse_responses(('\n'.join(sample_lines[:-1])).encode())),
        reject_control('duplicate-stimulus', lambda: parse_responses(('\n'.join(bad_duplicate)).encode())),
        reject_control('rating-above-ten', lambda: parse_responses(('\n'.join(bad_rating)).encode())),
        reject_control('embedded-blank', lambda: parse_responses(('\n'.join(sample_lines[:2]+['']+sample_lines[2:])).encode())),
        reject_control('missing-session', lambda: aggregate(missing)),
    ]
    result = {
        'status':'PASS', 'archive_files':9, 'archive_bytes':sum(x['bytes'] for x in acquisition['files']),
        'source_commit':acquisition['commit'],
        'sato':{'doi':'10.7717/peerj.2751','license':'CC-BY-4.0','zip_sha256':sha(zip_path.read_bytes()),
                'session_files':72,'participants':36,'responses':2304,'cohort_means':96,'non_clean_witnesses':48,
                'mean_equals_midpoint':sum(row['relation_to_5']=='=' for row in means),
                'final_blank_files':sorted(final_blanks),'table_s1_exact_xyY_rows':32},
        'cie':cie, 'negative_controls_rejected':controls,
        'claim':'Primary bytes reproduce all 96 cohort means and 48 non-clean witnesses; all 32 Table S1 rows are bound under the DECLARED CONDITIONAL join; native CIE bytes match official metadata checksums and source licenses.',
        'limits':['Raw files have no explicit column codebook; parsing roles are supported assumptions.',
                  'Stimulus join is source-supported conditional, not author-explicit identity; Table3 conflict is retained.',
                  'No physical calibration, uncertainty, HCE, renderer or production admission.',
                  'Old ac6d965 derivation is not reconstructed; this is a new independent source closure.'],
    }
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n')
    print(json.dumps(result,ensure_ascii=False))


if __name__ == '__main__':
    main()
