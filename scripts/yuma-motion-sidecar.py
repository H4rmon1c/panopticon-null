#!/usr/bin/env python3
"""Materialize a source-bound county motion and attendance, or withhold it."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import sqlite3
ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('research',ROOT/'scripts/colorado-research.py');r=importlib.util.module_from_spec(spec);spec.loader.exec_module(r)
SCHEMA='pnull-yuma-motion-v1'; REF='us-co-county-08125'; RECORD='co-document-7f8320f04b37d2c99c52bcf4'
URL='https://yumacounty.net/wp-content/uploads/2025/08/Minutes2025.06.30_r.pdf'
PDF='3da13c5d09feb8704be698ec20228ff6370b49df61aa47e96d0ed16ea91c81a3'
INDEX='https://yumacounty.net/commissioners/'
INDEX_SHA='e095b6a63ae6dd2e4e065dc1c500787d08ab767a82215169d02dbf9813eb7ca9'
TEXT='ae8c0d74a145996a6f8baa2bd105ea67c823814238e005d8325d07aae852b7c9'
LINES={4:'YUMA COUNTY COMMISSIONERS',5:'June 30, 2025',
 7:'On June 30,2025 at 8:00 a.m., the Yuma County Board of Commissioners met in regular session. Chairman',
 8:'Adam Gates, Commissioner Scott Weaver, Commissioner Mike Leerar, (via phone), and Administrator',
 9:'Brandi Ritchey were present throughout the day. The Commissioners reviewed and signed accounts',
 65:'Sheriffs Office',66:'Undersheriff Larry Gilliland and Office Manager Stacy Zink met with the Commissioners to discuss a new',
 67:'body worn camera contract. The new body worn camera contract shows annual savings of approximately',
 73:'• Commissioner Weaver moved to approve the Body Worn Camera contract with Axon.',
 74:'Commissioner Leerar seconded the motion which passed unanimously.',75:'Clerks Office'}
ATTENDANCE=[{'name':'Adam Gates','role':'Chairman','individual_vote':'Not individually recorded'},
 {'name':'Scott Weaver','role':'Commissioner','individual_vote':'Not individually recorded'},
 {'name':'Mike Leerar','role':'Commissioner (via phone)','individual_vote':'Not individually recorded'}]

def receipt_digest(data_dir,url,digest):
    found=[]
    for path in (data_dir/'observations').iterdir():
        try:
            raw=r.read_blob(path,65536);item=json.loads(raw)
            if path.name==r.sha(raw)+'.json' and r.receipt_valid(item,url,digest):found.append(path.stem)
        except (OSError,ValueError,TypeError,KeyError):continue
    if not found:raise ValueError('source receipt unavailable')
    return min(found)

def materialize(data_dir,db):
    row=db.execute('SELECT state,candidate_json,result_json FROM document_candidates WHERE jurisdiction_ref=? AND url=?',(REF,URL)).fetchone()
    if not row or row[0]!='matched':raise ValueError('current source unavailable')
    current=json.loads(row[1]);result=json.loads(row[2]);candidate=result['candidate'];receipt=result['receipt']
    if (result.get('document_sha256')!=PDF or result.get('extracted_sha256')!=TEXT or candidate.get('url')!=URL or
        candidate.get('index_url')!=INDEX or candidate.get('index_sha256')!=INDEX_SHA or candidate.get('anchor_index')!=75 or
        current.get('url')!=URL or current.get('index_url')!=INDEX or 'yumacounty.net' not in r.discovery_hosts(data_dir,db,REF) or
        not r.receipt_valid(receipt,URL,PDF)):
        raise ValueError('source identity differs')
    raw=(json.dumps(receipt,sort_keys=True,separators=(',',':'))+'\n').encode();receipt_sha=r.sha(raw)
    if r.read_blob(data_dir/'observations'/(receipt_sha+'.json'),65536)!=raw:raise ValueError('document receipt differs')
    index_receipt=receipt_digest(data_dir,INDEX,INDEX_SHA)
    current_index_receipt=receipt_digest(data_dir,INDEX,current['index_sha256'])
    r.checked_document_candidate(data_dir,URL,candidate);r.checked_document_candidate(data_dir,URL,current)
    if not r.source_blob(data_dir,PDF,20*1024*1024).startswith(b'%PDF-'):raise ValueError('source is not a PDF')
    text=r.read_blob(data_dir/'extracted'/TEXT,5*1024*1024)
    if r.sha(text)!=TEXT:raise ValueError('extracted digest differs')
    lines=text.decode('utf8').splitlines()
    if any(n>len(lines) or lines[n-1]!=v for n,v in LINES.items()):raise ValueError('meeting, motion or attendance differs')
    return {'schema_version':SCHEMA,'state':'available','research_record':RECORD,'jurisdiction_ref':REF,'jurisdiction':'Yuma County',
      'meeting_date':'2025-06-30','body':'Board of County Commissioners',
      'motion':'Approve a body-worn-camera contract with Axon','outcome':'Motion passed unanimously',
      'attendance':ATTENDANCE,'notice':'The minutes are a summary, not a verbatim record. Individual votes, contract execution, delivery, deployment and office terms are not established by this record.',
      'source':{'url':URL,'sha256':PDF,'text_sha256':TEXT,'attendance_page':1,'motion_page':2,
        'attendance_locator':'extracted lines 4–9','motion_locator':'extracted lines 65–74',
        'index_url':INDEX,'index_sha256':INDEX_SHA,'index_anchor':75,'index_receipt_sha256':index_receipt,
        'current_index_sha256':current['index_sha256'],'current_index_anchor':current['anchor_index'],
        'current_index_receipt_sha256':current_index_receipt,'receipt_sha256':receipt_sha,'captured_at':receipt['retrieved_at']}}

def require_research_binding(snapshot):
    rows=[x for x in snapshot['records'] if x.get('id')==RECORD]
    if (len(rows)!=1 or rows[0].get('kind')!='meeting_document' or rows[0].get('historical') is not False or
        rows[0].get('jurisdiction_ref')!=REF or rows[0].get('source_url')!=URL or rows[0].get('source_sha256')!=PDF):
        raise ValueError('research source unavailable or changed')

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--data-dir',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--research-snapshot',type=Path);a=p.parse_args()
    if not a.data_dir.is_absolute() or not a.output.is_absolute():p.error('absolute paths required')
    try:
        with sqlite3.connect((a.data_dir/'research.sqlite3').as_uri()+'?mode=ro',uri=True) as db:
            db.execute('BEGIN');result=materialize(a.data_dir,db);db.rollback()
        if a.research_snapshot is not None:
            if not a.research_snapshot.is_absolute():raise ValueError('absolute snapshot required')
            require_research_binding(json.loads(r.read_blob(a.research_snapshot,8*1024*1024)))
    except (OSError,ValueError,TypeError,KeyError,sqlite3.Error):result={'schema_version':SCHEMA,'state':'not_available'}
    a.output.parent.mkdir(parents=True,exist_ok=True);tmp=a.output.with_name('.'+a.output.name+f'.{os.getpid()}.tmp')
    fd=os.open(tmp,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o644)
    try:
        with os.fdopen(fd,'w') as f:json.dump(result,f,separators=(',',':'));f.write('\n');f.flush();os.fsync(f.fileno())
        os.replace(tmp,a.output)
    finally:
        if tmp.exists():tmp.unlink()
if __name__=='__main__':main()
