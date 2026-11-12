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
SCHEMA='pnull-hudson-motion-v1'; REF='us-co-place-0837820'; RECORD='co-document-84c83a2e5166b057153d3d7d'
URL='https://hudsonco.gov/AgendaCenter/ViewFile/Minutes/_09162026-780'
PDF='7cdad6608a7efa105bec154ee97487046b274f731a070e858bfffb554b185ef7'
INDEX='https://hudsonco.gov/AgendaCenter'
INDEX_SHA='3af10c7da90422d799051423e424509b3fa38d320a188b6bb77ba5e5a7aa3b5e'
TEXT='e1c15a02b078b5b4f5bc6d86dfe8d6c410f3567818b4efcfbb31dd852c2a6953'
ROSTER='ROLL CALL Present Mayor Joe Hammock (Online) Mayor Pro Tem Don Post Councilmember Lisa Buesgens Councilmember Matt Cole Councilmember Candace Nolf Councilmember Zach Reyes (Online) Councilmember Rachel Thwaites Absent None'
SCOPE='RESOLUTION NO. 26-51: A RESOLUTION OF THE TOWN COUNCIL OF THE TOWN OF HUDSON DISCONTINUING SERVICES WITH FLOCK GROUP, INC., FOR AUTOMATIC LICENSE PLATE DETECTION'
MOTION='Councilmember Cole made a motion, seconded by Councilmember Thwaites to approve Resolution No. 26-51. The vote was as follows: Aye: Mayor Hammock, Mayor Pro Tem Post, Councilmembers Cole, Nolf, Buesgens, and Thwaites Nay: None Mayor Hammock declared the motion carried.'
VOTES=[{'name':'Joe Hammock','role':'Mayor','position':'Aye'}, {'name':'Don Post','role':'Mayor Pro Tem','position':'Aye'}, {'name':'Matt Cole','role':'Councilmember','position':'Aye'}, {'name':'Candace Nolf','role':'Councilmember','position':'Aye'}, {'name':'Lisa Buesgens','role':'Councilmember','position':'Aye'}, {'name':'Rachel Thwaites','role':'Councilmember','position':'Aye'}]

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
        candidate.get('index_url')!=INDEX or candidate.get('index_sha256')!=INDEX_SHA or candidate.get('anchor_index')!=107 or
        current.get('url')!=URL or current.get('index_url')!=INDEX or 'hudsonco.gov' not in r.discovery_hosts(data_dir,db,REF) or
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
    pages=text.decode('utf8').splitlines()
    if len(pages)!=5 or pages[0].count(ROSTER)!=1 or pages[3].count(SCOPE)!=1 or pages[3].count(MOTION)!=1 or pages[3].index(SCOPE)>pages[3].index(MOTION):raise ValueError('roster, motion or vote differs')
    return {'schema_version':SCHEMA,'state':'available','research_record':RECORD,'jurisdiction_ref':REF,'jurisdiction':'Hudson',
      'meeting_date':'2026-09-16','body':'Town Council',
      'motion':'Approve Resolution 26-51 directing nonrenewal of Flock automatic license-plate-detection services','outcome':'Motion carried; six recorded ayes, no nays', 'motion_details':'Councilmember Matt Cole moved; Councilmember Rachel Thwaites seconded.',
      'votes':VOTES,'unrecorded_vote':'Councilmember Zach Reyes was present at roll call; his vote on this motion is not recorded in these minutes.', 'notice':'This was a nonrenewal decision, not approval of the original contract. Delivery of the notice, actual termination or removal of equipment, current service status and office terms are not established by this record.',
      'source':{'url':URL,'sha256':PDF,'text_sha256':TEXT,'roster_page':1,'motion_page':4,
        'attendance_locator':'OCR page 1','motion_locator':'OCR page 4',
        'index_url':INDEX,'index_sha256':INDEX_SHA,'index_anchor':107,'index_receipt_sha256':index_receipt,
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
