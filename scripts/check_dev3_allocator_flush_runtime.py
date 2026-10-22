#!/usr/bin/env python3
"""Select new batch-one flush cases only; bind inputs/binary and preserve failures.

Mandatory and S5 are disjoint selections. --case repeats individual IDs, so a
repair need not rerun successful unchanged sibling cases. No build is performed.
"""
import argparse
from collections import Counter
import csv
import io
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

sys.dont_write_bytecode = True
from check_dev0 import digest, source_fingerprint
from check_dev3_allocation_stack import wait_process_group
from comm_campaign import write_json

PREFIX='storage::wal_io::bootstrap::doublewrite_install::mutable_storage::allocator_flush::tests::'
SELECTORS={'FN':'flush_fn_normal','FA':'flush_fa_admission','FB':'flush_fb_before_fault',
           'FP':'flush_fp_partial','FU':'flush_fu_unknown','FC':'flush_fc_cleanup','FR':'flush_fr_resources',
           'S5':'flush_s5_batch_one_expanded'}
COUNTS={'FN':6,'FA':13,'FB':31,'FP':11,'FU':6,'FC':6,'FR':3}
BASE='tests/fixtures/dev3-allocator-flush-one/'
FIXTURE=BASE+'mandatory-cases.tsv'
EXPANDED=BASE+'expanded-cases.tsv'
MANIFEST=BASE+'runtime-manifest.json'
ORACLE=BASE+'manifest.json'
BUDGET_SECONDS=120
S5_BUDGET_SECONDS=1200
BUILD_COMMAND=['cargo','test','-p','xadb-engine','--lib','--all-features','--no-run','--locked','--offline','--message-format=json']
CHECKER_NAMES=('check_dev3_allocator_flush_runtime.py','test_check_dev3_allocator_flush_runtime.py',
               'dev3_allocator_flush_oracle.py','test_dev3_allocator_flush_oracle.py',
               'check_dev3_allocation_stack.py','check_dev0.py','comm_campaign.py')


def mandatory_cases(text):
    reader=csv.DictReader(io.StringIO(text),delimiter='\t')
    if reader.fieldnames!=['case_id','group','scenario','expected','status']:
        raise ValueError('mandatory header')
    groups={group:[] for group in COUNTS}
    for row in reader:
        if None in row or any(not v for v in row.values()) or row['status']!='INPUT_NOT_RUN':
            raise ValueError('mandatory row shape/status')
        identifier=row['case_id']; group=identifier[:2]
        if group not in groups: raise ValueError('mandatory group')
        groups[group].append(identifier)
    if any(groups[g]!=[f'{g}{i:02}' for i in range(1,n+1)] for g,n in COUNTS.items()):
        raise ValueError('mandatory census/order/duplicates')
    return groups


def expanded_cases(text):
    reader=csv.DictReader(io.StringIO(text),delimiter='\t')
    if reader.fieldnames!=['id','batch_input','mode','cut','status']: raise ValueError('expanded header')
    rows=list(reader)
    expected=[]
    writes=(1,3,6,10,12,14,17,21,23,25,28)
    for profile in (1,2,3):
        for count,operation in ((1,'reserve'),(1,'cancel'),(8,'reserve'),(8,'cancel'),(63,'reserve'),(63,'cancel')):
            for mode,cuts in (('normal',(32,)),('after',range(1,32)),('short',writes),('interrupted',writes)):
                for cut in cuts:
                    expected.append({'id':f'S5-P{profile}-N{count}-{operation.upper()}-{mode.upper()}-{cut:02}',
                                     'batch_input':f'p{profile}-n{count}-{operation}','mode':mode,'cut':str(cut),'status':'S5_INPUT_NOT_RUN'})
    if rows!=expected: raise ValueError('expanded exact matrix differs')
    return [row['id'] for row in rows]


def select(groups,expanded,requested=None,expanded_requested=None,s5=False):
    if requested is not None and (s5 or expanded_requested is not None):
        raise ValueError('mandatory/S5 selections cannot mix')
    if s5 and expanded_requested is not None: raise ValueError('all-S5/individual selections cannot mix')
    eligible=expanded if s5 or expanded_requested is not None else [i for ids in groups.values() for i in ids]
    selected=(expanded_requested if expanded_requested is not None else requested)
    selected=list(eligible) if selected is None else selected
    if not selected or len(selected)!=len(set(selected)) or set(selected)-set(eligible):
        raise ValueError('empty/duplicate/unknown selected case')
    # Stable inventory order, regardless of CLI order; Rust must use this order too.
    selected=[i for i in eligible if i in set(selected)]
    result={}
    for identifier in selected:
        result.setdefault('S5' if identifier.startswith('S5-') else identifier[:2],[]).append(identifier)
    return result


def safe_file(root,name):
    if Path(name).is_absolute() or '..' in Path(name).parts: raise ValueError('unsafe fixture path')
    return root/name


def verify_inputs(root,manifest,normal_sha,expanded_sha):
    if (manifest.get('schema')!='xadb-dev3-allocator-flush-runtime-input-v1'
            or manifest.get('mandatory_cases')!=76 or manifest.get('s5_cases')!=972
            or manifest.get('group_counts')!=COUNTS
            or manifest.get('selectors')!={g:PREFIX+n for g,n in SELECTORS.items()}
            or manifest.get('normal_sha256')!=normal_sha or manifest.get('expanded_sha256')!=expanded_sha
            or manifest.get('mandatory_budget_seconds')!=BUDGET_SECONDS
            or manifest.get('s5_budget_seconds')!=S5_BUDGET_SECONDS
            or manifest.get('known_bad')!=19 or manifest.get('fa13_variants')!=[f'V{i:02}' for i in range(1,7)]):
        raise ValueError('runtime input manifest mismatch')
    oracle_path=root/ORACLE
    if digest(oracle_path)!=manifest.get('oracle_manifest_sha256'): raise ValueError('oracle identity')
    oracle=json.loads(oracle_path.read_text())
    if oracle.get('mandatory_cases')!=76 or oracle.get('s5_cases')!=972: raise ValueError('oracle census')
    identities={ORACLE:digest(oracle_path),MANIFEST:digest(root/MANIFEST)}
    for row in oracle['literals']:
        name=BASE+row['path']; path=safe_file(root,name)
        if path.stat().st_size!=row['bytes'] or digest(path)!=row['sha256']: raise ValueError('literal identity '+name)
        identities[name]=row['sha256']
    for row in oracle['inputs']:
        path=safe_file(root,row['path'])
        if path.stat().st_size!=row['bytes'] or digest(path)!=row['sha256']: raise ValueError('provenance input identity')
        identities[row['path']]=row['sha256']
    return identities


def parse_case_markers(stdout):
    prefix=r'^(?:test [\w:]+ \.\.\. )?'
    begins=re.findall(prefix+r'FLUSH_CASE_BEGIN\t([^\r\n]+)$',stdout,re.M)
    ends=re.findall(prefix+r'FLUSH_CASE\t([^\r\n]+)$',stdout,re.M)
    return begins,ends


def evaluate(stdout,process,group,expected):
    if process.get('timed_out') or process.get('exit_code')!=0: raise ValueError('runtime process failed/timed out')
    summaries=re.findall(r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; \d+ filtered out; finished in [\d.]+s$',stdout,re.M)
    if summaries!=[('1','0','0','0')]: raise ValueError('exact test success census')
    names=re.findall(r'^test ([\w:]+) \.\.\.',stdout,re.M)
    if names!=[PREFIX+SELECTORS[group]]: raise ValueError('wrong executed selector')
    begins,ends=parse_case_markers(stdout)
    if begins!=expected or ends!=expected: raise ValueError('case begin/end order/census')
    # Every completion must follow its own BEGIN and precede the next BEGIN.
    sequence=re.findall(r'^(?:test [\w:]+ \.\.\. )?FLUSH_CASE(_BEGIN)?\t([^\r\n]+)$',stdout,re.M)
    if sequence!=[pair for identifier in expected for pair in (('_BEGIN',identifier),('',identifier))]:
        raise ValueError('case marker order')
    completed=re.findall(r'^FLUSH_COMPLETE (\w+) (\d+)$',stdout,re.M)
    if completed!=[(group,str(len(expected)))]: raise ValueError('completion marker mismatch')
    variants=re.findall(r'^FLUSH_VARIANT\tFA13\t(V\d+)$',stdout,re.M)
    if variants!=([f'V{i:02}' for i in range(1,7)] if 'FA13' in expected else []):
        raise ValueError('FA13 variant census')
    negatives=re.findall(r'^FLUSH_ORACLE_NEGATIVES (\d+)$',stdout,re.M)
    if negatives!=(['19'] if 'FR03' in expected else []): raise ValueError('known bad witness census')
    # Reject malformed/foreign protocol rows rather than silently ignore typos.
    allowed=(r'FLUSH_CASE_BEGIN\t[^\r\n]+',r'FLUSH_CASE\t[^\r\n]+',r'FLUSH_COMPLETE \w+ \d+',
             r'FLUSH_VARIANT\tFA13\tV\d+',r'FLUSH_ORACLE_NEGATIVES \d+')
    for line in stdout.splitlines():
        line=re.sub(r'^test [\w:]+ \.\.\. ','',line)
        if line.startswith('FLUSH_') and not any(re.fullmatch(pattern,line) for pattern in allowed):
            raise ValueError('unknown or malformed flush marker')
    return {'status':'PASS','case_count':len(ends),'cases':ends,'independent_negatives':19 if 'FR03' in expected else 0}


def verify_build(receipt,root,binary,source_sha,binary_sha):
    expected={'status':'PASS','source_root':str(root),'binary':str(binary),'source_sha256':source_sha,
              'binary_sha256':binary_sha,'command':BUILD_COMMAND,'exit_code':0}
    if any(receipt.get(k)!=v for k,v in expected.items()): raise ValueError('build receipt mismatch')


def run(root,binary,output,source_sha,binary_sha,normal_sha,expanded_sha,build_receipt,
        requested=None,expanded_requested=None,s5=False):
    root,binary,output=root.resolve(),binary.resolve(),output.resolve()
    if source_fingerprint(root)!=source_sha or digest(binary)!=binary_sha: raise ValueError('source/binary identity')
    if digest(root/FIXTURE)!=normal_sha or digest(root/EXPANDED)!=expanded_sha: raise ValueError('inventory identity')
    groups=mandatory_cases((root/FIXTURE).read_text()); expanded=expanded_cases((root/EXPANDED).read_text())
    selected=select(groups,expanded,requested,expanded_requested,s5)
    identities=verify_inputs(root,json.loads((root/MANIFEST).read_text()),normal_sha,expanded_sha)
    build_receipt=build_receipt.resolve()
    verify_build(json.loads(build_receipt.read_text()),root,binary,source_sha,binary_sha)
    build_sha=digest(build_receipt)
    scripts=Path(__file__).resolve().parent
    checker={str(scripts/name):digest(scripts/name) for name in CHECKER_NAMES}
    if any(digest(root/'scripts'/Path(name).name)!=value for name,value in checker.items()):
        raise ValueError('executing checker differs from frozen export')
    all_ids=expanded if 'S5' in selected else [i for ids in groups.values() for i in ids]
    selected_ids=[i for ids in selected.values() for i in ids]
    budget=S5_BUDGET_SECONDS if 'S5' in selected else BUDGET_SECONDS
    output.mkdir(parents=True,exist_ok=False)
    report={'status':'RUNNING','scope':'DEV3 allocator batch-one flush '+('S5 expanded' if 'S5' in selected else 'mandatory changed-code'),
            'source_root':str(root),'source_sha256':source_sha,'binary':str(binary),'binary_sha256':binary_sha,
            'build_receipt':str(build_receipt),'build_receipt_sha256':build_sha,'fixture_identities':identities,
            'executing_checker_files':checker,'selected_cases':selected_ids,'not_selected':[i for i in all_ids if i not in selected_ids],
            'runtime_budget_seconds':budget,'runtime_elapsed_seconds':0.0,'native_status':'SEPARATE_NOT_RUN',
            'wal_status':'SEPARATE_NOT_RUN','expanded_status':'RUNNING' if 'S5' in selected else 'NOT_RUN','groups':[]}
    environment=os.environ.copy()
    for key in list(environment):
        if key.startswith(('XADB_','XADBTEST_','RUST_TEST_')):environment.pop(key)
    environment['PYTHONDONTWRITEBYTECODE']='1'
    try:
        for group,ids in selected.items():
            remaining=budget-report['runtime_elapsed_seconds']
            if remaining<=0: raise RuntimeError('aggregate runtime budget exhausted')
            directory=output/group;directory.mkdir()
            command=[str(binary),'--exact',PREFIX+SELECTORS[group],'--nocapture','--test-threads=1']
            if group=='S5':command.append('--ignored')
            environment['XADB_FLUSH_CASES']=','.join(ids)
            process={'command':command,'case_environment':environment['XADB_FLUSH_CASES'],
                     'exit_code':None,'timed_out':False,'timeout_seconds':remaining}
            row={'group':group,'status':'RUNNING','selected_cases':ids,'process':process};report['groups'].append(row)
            started=time.monotonic()
            try:
                with (directory/'stdout.log').open('wb') as stdout,(directory/'stderr.log').open('wb') as stderr:
                    with subprocess.Popen(command,cwd=root,env=environment,stdout=stdout,stderr=stderr,start_new_session=True) as child:
                        process['pid']=child.pid;wait_process_group(child,process,remaining)
            finally:
                process['elapsed_seconds']=time.monotonic()-started
                report['runtime_elapsed_seconds']+=process['elapsed_seconds']
                row['logs']={str(p):digest(p) for p in directory.iterdir() if p.is_file()}
                if (directory/'stdout.log').exists():
                    row['observed_begin_markers'],row['observed_success_markers']=parse_case_markers((directory/'stdout.log').read_text())
            row.update(evaluate((directory/'stdout.log').read_text(),process,group,ids))
            if report['runtime_elapsed_seconds']>budget:raise RuntimeError('aggregate runtime budget exceeded')
        if (source_fingerprint(root)!=source_sha or digest(binary)!=binary_sha or digest(build_receipt)!=build_sha
                or any(digest(root/name)!=value for name,value in identities.items())
                or any(digest(Path(name))!=value for name,value in checker.items())):
            raise ValueError('source/binary/fixture/build/checker changed during execution')
        report['status']='PASS'
        if 'S5' in selected:report['expanded_status']='PASS_SELECTED_CASES'
    except BaseException as error:
        report.update(status='FAIL',error=str(error))
        for row in report['groups']:
            if row['status']=='RUNNING':row['status']='FAIL'
        if 'S5' in selected:report['expanded_status']='FAIL_OR_INCOMPLETE'
        raise
    finally:
        began={i for row in report['groups'] for i in row.get('observed_begin_markers',[])}
        report['not_run']=[i for i in selected_ids if i not in began]
        report['successful_marker_only_not_group_pass']=[i for row in report['groups'] if row['status']!='PASS' for i in row.get('observed_success_markers',[])]
        write_json(output/'result.json',report)
    return report


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for key in ('root','binary','output','build-receipt'):parser.add_argument('--'+key,type=Path,required=True)
    for key in ('source-sha256','binary-sha256','normal-sha256','expanded-sha256'):parser.add_argument('--'+key,required=True)
    parser.add_argument('--case',action='append')
    parser.add_argument('--expanded-case',action='append')
    parser.add_argument('--s5',action='store_true')
    args=parser.parse_args()
    report=run(args.root,args.binary,args.output,args.source_sha256,args.binary_sha256,args.normal_sha256,
               args.expanded_sha256,args.build_receipt,args.case,args.expanded_case,args.s5)
    print(json.dumps({'status':report['status'],'selected_cases':len(report['selected_cases']),'seconds':report['runtime_elapsed_seconds']}))


if __name__=='__main__':main()
