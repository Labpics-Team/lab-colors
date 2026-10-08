#!/usr/bin/env python3
"Независимая точная проверка nominal-таблицы; только стандартная библиотека."
from fractions import Fraction as F
from pathlib import Path
from functools import lru_cache
from math import lcm,gcd
from bisect import bisect_left,bisect_right
import json,hashlib,gzip,struct,csv,sys,time,collections
ROOT=Path(__file__).resolve().parent
IN=ROOT/'input'
OUT=ROOT/sys.argv[1]
START=time.monotonic()
def require(ok,label):
    if not ok: raise ValueError(label)
def identity_gate(actual,expected):require(actual==expected,'identity mismatch')
def observable_gate(actual,expected):require(actual==expected,'observable mismatch')
def canonical_pair_gate(pair):require(pair[0]<=pair[1] or tuple(pair)==(255,0),'noncanonical empty')
def strict_policy_gate(B,outcome):require(outcome==(B>T),'strict policy mismatch')
def rejects(callback,label):
    try:callback()
    except ValueError as error:
        require(str(error)==label,'unexpected mutation failure '+str(error));return str(error)
    raise ValueError('mutant survived '+label)
def rat(x):
    return F(int(x['numerator']),int(x['denominator'])) if isinstance(x,dict) else F(x)
def read(name):return json.loads((IN/name).read_text(encoding='utf-8'))
def sha(name):return hashlib.sha256((IN/name).read_bytes()).hexdigest()
def dot(a,b):return sum(x*y for x,y in zip(a,b))
def cross(a,b):return (a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0])
def progress(stage,**kwargs):print(json.dumps({'stage':stage,'seconds':round(time.monotonic()-START,3),**kwargs}),flush=True)
p=read('profile-candidate.json');f=read('frontier-candidate.json');full=read('intervals-full.json');ref=read('intervals-refined.json');bridge=read('bridge.json');mut=read('mutation-corpus.json')
require(sha('intervals-refined.raw')=='cf42419977850c435ede3e5be05a0a74b14ad98255418aac7d396f5bb501550e','external table identity')
identity_gate(sha('bridge.json'),'cd5f1839179166e8f898fb03bd26a6c8e85efc50e60f480393f88ccdfc5ec2ab')
source_names={'cmf':'cie-cmf.csv','spd':'cie-d65.csv','join':'join.json','bridge':'bridge.json','witnesses':'witnesses.json'}
for key,name in source_names.items():require(sha(name)==p['source_pins'][key]['sha256'],'source pin '+key)
require(f['profile_sha256']==sha('profile-candidate.json'),'frontier profile pin')
for key,name in {'profile':'profile-candidate.json','frontier':'frontier-candidate.json','bridge':'bridge.json'}.items():require(full['pins'][key]==sha(name),'full pin '+key)
for name,h in ref['input_sha256'].items():
    if name!='interval_geometry.py':require(sha(name)==h,'refined input pin '+name)
for name,h in mut['input_sha256'].items():require(sha(name)==h,'mutation input pin '+name)
for name,data in full['files'].items():require(sha(name)==data['sha256'] and (IN/name).stat().st_size==data['bytes'],'file pin '+name)
require(sha('intervals-full-primal.jsonl.gz')==full['primal_proof']['sha256'],'primal pin')
cmf={int(r[0]):tuple(F(x) for x in r[1:4]) for r in csv.reader((IN/'cie-cmf.csv').open(encoding='utf-8'))}
spd={int(r[0]):F(r[1]) for r in csv.reader((IN/'cie-d65.csv').open(encoding='utf-8'))}
require([r['wavelength_nm'] for r in p['generators']]==list(range(360,781)),'complete spectral domain')
V=[tuple(rat(x) for x in row['xyz']) for row in p['generators']]
require(V==[tuple(spd[n]*x for x in cmf[n]) for n in range(360,781)],'native decimal generator multiplication')
D=lcm(*(x.denominator for v in V for x in v))
W=[tuple(int(x*D) for x in v) for v in V]
require(all(x>=0 for v in W for x in v),'nonnegative spectral generators')
decrows=sorted([r for r in bridge['rows'] if r['group']=='decode'],key=lambda r:r['index'])
matrows=sorted([r for r in bridge['rows'] if r['group']=='matrix'],key=lambda r:r['index'])
require([r['index'] for r in decrows]==list(range(256)) and [r['index'] for r in matrows]==list(range(9)),'bridge index completeness')
for row in decrows+matrows:
    require(rat(row['dyadic'])==F.from_float(struct.unpack('>d',bytes.fromhex(row['bits_hex']))[0]),'binary64 dyadic '+str(row['index']))
decode=[rat(r['dyadic']) for r in decrows];matrix=[rat(r['dyadic']) for r in matrows]
require(decode[0]==0 and decode[-1]==1 and all(a<b for a,b in zip(decode,decode[1:])),'strict decode ordering')
require(rat(bridge['scale'])==100,'scale100')
require(dot(matrix[:3],cross(matrix[3:6],matrix[6:]))!=0,'invertible output matrix')
Q={k:tuple(rat(x) for x in v) for k,v in p['queries'].items()}
join=read('join.json')['rows'];require(sorted(r['raw_stimulus_id'] for r in join)==list(range(32)),'full source query IDs')
for row in join:
    xy=row['xyY_decimal_tokens'];x,y,Y=map(F,(xy['x'],xy['y'],xy['Y']))
    require(Q['Sato:'+str(row['raw_stimulus_id'])]==(x*Y/y,Y,(1-x-y)*Y/y),'source XYZ formula')
for c,k in enumerate(('red','green','blue')):require(Q['primary:'+k]==tuple(100*matrix[3*i+c] for i in range(3)),'primary output binding '+k)
require(set(p['positive_ray_certificates'])==set(Q),'ray certificate completeness')
for key,c in p['positive_ray_certificates'].items():
    idx=[n-360 for n in c['wavelengths_nm']];rho=[rat(x) for x in c['reflectance']];mu=rat(c['mu'])
    require(len(set(idx))==len(idx) and len(idx)==len(rho) and all(0<=i<421 for i in idx),'ray indexing '+key)
    require(mu>0 and all(0<=x<=1 for x in rho),'positive bounded ray '+key)
    require(tuple(sum(V[i][j]*x for i,x in zip(idx,rho)) for j in range(3))==tuple(mu*x for x in Q[key]),'ray equality '+key)
require(set(f['certificates'])=={'Sato:'+str(i) for i in range(32)},'32 frontier IDs')
brightness={}
for key,c in f['certificates'].items():
    q=Q[key];require(all(x>0 for x in q),'strictly positive source XYZ')
    rho=[rat(c['primal']['default_reflectance'])]*421;seen=set()
    for r in c['primal']['nonzero']:
        i=r['wavelength_nm']-360;require(0<=i<421 and i not in seen,'frontier primal ID');seen.add(i);rho[i]=rat(r['r'])
    require(all(0<=x<=1 for x in rho),'frontier primal box '+key)
    X=tuple(sum(v[j]*r for v,r in zip(V,rho)) for j in range(3))
    mu=X[1]/q[1];require(mu>0 and X==tuple(mu*x for x in q),'frontier ray feasibility '+key)
    x,y,z=(u/sum(q) for u in q);lam=list(map(rat,c['dual_lambda']))
    dual=sum(max(F(0),v[1]-lam[0]*(y*v[0]-x*v[1])-lam[1]*(y*v[2]-z*v[1])) for v in V)
    require(dual==X[1]==rat(c['Y_oc_max']),'exact frontier primal dual objective '+key)
    require(rat(c['B'])==1/mu,'brightness normalization '+key);brightness[key]=1/mu
witnesses=read('witnesses.json')['rows'];keys={(w['CohortId'],w['StimulusId']) for w in witnesses}
require(len(keys)==len(witnesses)==48,'canonical witness uniqueness')
require(keys=={(w['CohortId'],w['StimulusId']) for w in p['canonical_witness_keys']},'witness set binding')
require(all(rat(w['mean'])>=5 for w in witnesses),'witness canonical >=5')
T=max(brightness['Sato:'+str(i)] for cohort,i in keys)
require(T>0 and T==rat(f['threshold']['T_policy']),'independent max threshold')
strict_policy_gate(T,f['threshold']['equality']!='reject')
for cohort,t in f['threshold']['cohort_T'].items():require(rat(t)==max(brightness['Sato:'+str(i)] for c,i in keys if c==cohort),'cohort threshold '+cohort)
require({(w['CohortId'],w['StimulusId']) for w in f['threshold']['maximizing_keys']}=={(c,i) for c,i in keys if brightness['Sato:'+str(i)]==T},'maximizing keys')
progress('profile_frontier_verified',generators=len(V),positive_rays=len(Q),frontiers=len(brightness))
# Масштабирование в целочисленную решётку не меняет ни одно неравенство.
A=tuple(tuple(D*100*matrix[3*j+i]/T for j in range(3)) for i in range(3))
@lru_cache(None)
def face(pair,sign):
    require(len(pair)==2 and pair[0]!=pair[1] and all(360<=n<=780 for n in pair) and sign in (-1,1),'face address')
    normal=cross(W[pair[0]-360],W[pair[1]-360]);div=gcd(*normal);require(div!=0,'nondegenerate face')
    n=tuple(sign*x//div for x in normal)
    dots=[dot(n,v) for v in W];free=[i for i,d in enumerate(dots) if d==0]
    fixed=tuple(sum(v[j] for v,d in zip(W,dots) if d>0) for j in range(3));h=sum(max(0,d) for d in dots)
    require(dot(n,fixed)==h,'support maximum exact')
    coeff=tuple(dot(n,v) for v in A)
    require(coeff[2]!=0,'transverse face')
    return n,free,fixed,h,coeff

def face_record(rec):
    obj=face(tuple(rec['pair_nm']),rec['sign']);require(len(obj[1])==rec['free_count'],'face free-count');return obj
faces=[face_record(rec) for rec in full['faces']]
progress('support_faces_verified',face_count=len(faces))
def boundary(fc,c,threshold=T,mat=matrix):
    n,free,fixed,h,coeff=fc;r,g=divmod(c,256)
    if threshold!=T or mat!=matrix:coeff=tuple(dot(n,tuple(D*100*mat[3*j+i]/threshold for j in range(3))) for i in range(3))
    return (h-coeff[0]*decode[r]-coeff[1]*decode[g])/coeff[2]
def endpoint(fc,c,rr,threshold=T,mat=matrix):
    n,free,fixed,h,coeff=fc;rs=list(map(rat,rr));require(len(rs)==len(free),'endpoint reflectance count')
    require(all(0<=v<=1 for v in rs),'endpoint reflectance box')
    beta=boundary(fc,c,threshold,mat);r,g=divmod(c,256)
    actual=tuple(fixed[j]+sum(W[i][j]*v for i,v in zip(free,rs)) for j in range(3))
    query=tuple(D*100/threshold*(mat[3*j]*decode[r]+mat[3*j+1]*decode[g]+mat[3*j+2]*beta) for j in range(3))
    return beta,tuple((actual[j]-query[j])/D for j in range(3))
def exact_endpoint_gate(fc,c,rr,threshold=T,mat=matrix):
    beta,residual=endpoint(fc,c,rr,threshold,mat)
    require(residual==(0,0,0),'endpoint equality')
    return beta,residual
corrections={e['column']:e for e in ref['corrections']}
require(len(corrections)==len(ref['corrections'])==14,'14 unique corrections')
require(set(corrections)==set(ref['initial_failed_columns'])=={256*r+g for r,g in full['failures']},'correction set exactly initial failures')
require(not ref['unresolved'],'no unresolved corrections')
raw=(IN/'intervals-refined.raw').read_bytes();oldraw=(IN/'intervals-full.raw').read_bytes();facemap=(IN/'intervals-full-faces.bin').read_bytes()
require(len(raw)==len(oldraw)==131072 and len(facemap)==524288,'binary lengths')
derived=bytearray();counts=collections.Counter(empty=0,singleton=0,full=0,interval=0);input_kinds=collections.Counter();seen=set();endpoint_count=0;outcomes=collections.Counter();neutral_rejected=0
bounds=[];saved_endpoint=None
with gzip.open(IN/'intervals-full-primal.jsonl.gz','rt', encoding='utf-8') as rows:
 for line in rows:
    cert=json.loads(line);c=cert['column'];require(type(c) is int and c==len(seen) and 0<=c<65536 and c not in seen,'ordered unique total column');seen.add(c);input_kinds[cert['kind']]+=1
    require(tuple(cert['faces'])==struct.unpack_from('<II',facemap,8*c),'face map column '+str(c))
    require(len(cert['faces'])==2 and all(type(i) is int and 0<=i<len(faces) for i in cert['faces']),'valid face index')
    fs=[faces[i] for i in cert['faces']]
    if c in corrections:
        require(cert['kind']=='BoundaryUnproven','correction scope');e=corrections[c];require(len(e['endpoints'])==2,'two corrected endpoints')
        fs=[face_record(x['face']) for x in e['endpoints']];rr=[x['free_reflectance'] for x in e['endpoints']]
    else:
        require(cert['kind'] in ('ExactInterval','Separated'),'recognized complete kind');rr=cert.get('free_reflectance')
    require(fs[0][4][2]<0<fs[1][4][2],'lower upper support orientation '+str(c))
    low,high=(boundary(fc,c) for fc in fs)
    if cert['kind']=='Separated':
        require(c not in corrections and low>high,'strict separation '+str(c));pair=(255,0);counts['empty']+=1
    else:
        require(low<=high and len(rr)==2,'ordered interval '+str(c))
        for j,fc in enumerate(fs):
            beta,residual=exact_endpoint_gate(fc,c,rr[j]);endpoint_count+=1
            if c in corrections:require(beta==F(corrections[c]['endpoints'][j]['bound']),'saved correction bound')
        lo=bisect_left(decode,low);hi=bisect_right(decode,high)-1
        pair=(lo,hi) if lo<=hi else (255,0)
        counts['empty' if lo>hi else 'singleton' if lo==hi else 'full' if (lo,hi)==(0,255) else 'interval']+=1
        if c==1:saved_endpoint=(fs[1],rr[1])
    canonical_pair_gate(raw[2*c:2*c+2])
    require(tuple(raw[2*c:2*c+2])==pair,'derived raw bytes column '+str(c))
    if c in corrections:require(tuple(corrections[c]['raw_pair'])==pair,'correction raw bytes')
    else:require(raw[2*c:2*c+2]==oldraw[2*c:2*c+2],'outside correction bytes unchanged')
    derived.extend(pair);bounds.append((low,high))
    r,g=divmod(c,256);lo,hi=pair
    if r==g and lo<=r<=hi:neutral_rejected+=1
    # Прямое членство использует rational endpoints; потребитель читает только байты.
    for b,d in enumerate(decode):
        direct_reject=(r,g,b)!=(b,b,b) and low<=d<=high
        table_reject=not(r==g==b) and raw[2*c]<=b<=raw[2*c+1]
        expected=('Rejected',lo,hi) if direct_reject else ('Accepted',)
        actual=('Rejected',raw[2*c],raw[2*c+1]) if table_reject else ('Accepted',)
        observable_gate(actual,expected)
        outcomes['rejected' if direct_reject else 'accepted']+=1
    if c%8192==8191:progress('columns_verified',columns=c+1,endpoints=endpoint_count)
require(seen==set(range(65536)),'full 65536 coverage')
require(dict(counts)==ref['counts'],'derived column counts')
require(outcomes['rejected']==ref['rejected_chromatic'] and outcomes['accepted']-256==ref['accepted_chromatic'],'derived chromatic outcome counts')
require(neutral_rejected==ref['neutral_rejected_before_union'],'neutral union count')
require(bytes(derived)==raw,'entire byte equality')
(OUT/'derived.raw').write_bytes(derived)
progress('full_cube_verified',outcomes=dict(outcomes),counts=dict(counts))
def classify(rgb,table=derived,neutral='exact'):
    r,g,b=rgb;lo,hi=table[2*(r*256+g):2*(r*256+g)+2]
    if (neutral=='exact' and r==g==b) or (neutral=='wide' and r==g):return 'Accepted'
    return {'Rejected':[lo,hi]} if lo<=b<=hi else 'Accepted'
require(len(mut['cases'])==11 and len({x['id'] for x in mut['cases']})==11,'11 unique saved mutations')
mutation_results=[]
fc,rr=saved_endpoint
base_beta,base_residual=exact_endpoint_gate(fc,1,rr)
require(base_residual==(0,0,0) and base_beta==F(mut['baseline_witness']['beta']),'healthy endpoint mutation control')
for case in mut['cases']:
    k=case['id'];patch=case['patch'];w=case.get('witness',{});record={'id':k,'status':'TARGETED_REJECTION'}
    if k in ('threshold_numerator','threshold_denominator','output_matrix_identity'):
        mt=T;mm=matrix.copy()
        if 'threshold' in k:mt=F(patch['value'])
        else:mm[patch['matrix_index']]=F(patch['value'])
        beta,residual=endpoint(fc,1,rr,mt,mm)
        exact_endpoint_gate(fc,1,rr)
        record['gate_error']=rejects(lambda:exact_endpoint_gate(fc,1,rr,mt,mm),'endpoint equality')
        if 'beta' in w:require(beta==F(w['beta']),'saved mutated beta '+k)
        saved=tuple(F(x) for x in w['xyz_residual']);require(residual==saved or tuple(-x for x in residual)==saved,'saved nonzero residual '+k)
        record.update(reason='nonzero exact endpoint XYZ residual',computed_residual=[str(x) for x in residual])
    elif k=='strict_frontier_equality':
        B=brightness['Sato:'+str(w['stimulus'])];require(B==T==F(w['B'])==F(w['T']) and not B>T and B>=T,'strict equality target')
        strict_policy_gate(B,B>T)
        record['gate_error']=rejects(lambda:strict_policy_gate(B,B>=T),'strict policy mismatch')
        record['reason']='B equals T: strict predicate rejects, mutated non-strict predicate accepts'
    elif k=='output_binding_identity':
        identity_gate(sha('bridge.json'),'cd5f1839179166e8f898fb03bd26a6c8e85efc50e60f480393f88ccdfc5ec2ab')
        record['gate_error']=rejects(lambda:identity_gate(patch['bridge_sha256'],'cd5f1839179166e8f898fb03bd26a6c8e85efc50e60f480393f88ccdfc5ec2ab'),'identity mismatch')
        record['reason']='external expected bridge SHA mismatch'
    elif k=='empty_sentinel':
        altered=bytearray(derived);altered[patch['byte_offset']]=patch['value'];c=w['column'];pair=list(altered[2*c:2*c+2])
        require(list(derived[2*c:2*c+2])==w['expected'] and pair==w['mutant'] and pair[0]>pair[1] and pair!=[255,0],'noncanonical empty pair')
        canonical_pair_gate(derived[2*c:2*c+2])
        record['gate_error']=rejects(lambda:canonical_pair_gate(pair),'noncanonical empty')
        record['reason']='invalid empty encoding although membership booleans agree'
    else:
        rgb=w['rgb'];expected=classify(rgb);altered=bytearray(derived)
        if k=='rgb_order':
            require(patch['permutation']==['green','red','blue'],'saved RGB permutation');actual=classify([rgb[1],rgb[0],rgb[2]])
        elif k in ('interval_endpoint','table_bit'):
            pos=patch['byte_offset'];altered[pos]=(altered[pos]^patch['xor']) if 'xor' in patch else patch['value'];actual=classify(rgb,altered)
        elif k=='neutral_widening':actual=classify(rgb,neutral='wide')
        elif k=='neutral_omission':actual=classify(rgb,neutral='none')
        else:raise ValueError('unknown mutation '+k)
        require(expected==w['expected'] and actual==w['mutant'] and expected!=actual,'observable targeted mutation '+k)
        observable_gate(expected,expected)
        record['gate_error']=rejects(lambda:observable_gate(actual,expected),'observable mismatch')
        record.update(reason='observable decision or Rejected interval changed',expected=expected,mutant=actual)
    mutation_results.append(record)
    progress('mutation_rejected',id=k)
result={'verdict':'PASS','scope':'Exact finite nominal interval table and observable codec only','historical_upstream_candidate_commit':'d3224cd485701d7fa8448c19b0529f3931b03acb','historical_upstream_contract_commit':'25745723031e6ebda7111d97c426262392b67b36','table_sha256':hashlib.sha256(derived).hexdigest(),'verifier_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'arithmetic':'Exact integers and fractions.Fraction; no epsilon or floating optimizer','spectral_generators':len(V),'positive_ray_certificates':len(Q),'source_frontiers':len(brightness),'registered_faces':len(faces),'distinct_support_faces_checked':face.cache_info().currsize,'columns':len(seen),'input_certificate_kinds':dict(input_kinds),'corrections':len(corrections),'endpoint_primal_equalities':endpoint_count,'column_counts':dict(counts),'cube_outcomes':dict(outcomes),'cube_points':sum(outcomes.values()),'accepted_chromatic':outcomes['accepted']-256,'rejected_chromatic':outcomes['rejected'],'neutral_rejected_before_union':neutral_rejected,'mutations':mutation_results,'mutation_count':len(mutation_results),'runtime_seconds':time.monotonic()-START,'non_claims':['Human or perceptual validation','Physical applicability to displays','Continuous spectral model','Historical ac6 equivalence','Full REVIEW-01 readiness','Production admission or deployment']}
(OUT/'result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n', encoding='utf-8', newline='\n')
progress('PASS',table_sha256=result['table_sha256'],mutation_count=len(mutation_results))
