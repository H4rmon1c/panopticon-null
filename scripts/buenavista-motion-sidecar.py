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
SCHEMA='pnull-buenavista-motion-v1'; REF='us-co-place-0810105'; RECORD='co-document-70b769207d0e0c7d13953611'
URL='https://www.buenavistaco.gov/AgendaCenter/ViewFile/Minutes/_01282025-1883'
PDF='4af96edb75e01a5447b372128f379ad14416e7a23ec8a7b36f3f91096364353c'
INDEX='https://www.buenavistaco.gov/AgendaCenter/UpdateCategoryList?year=2025&catID=1&startDate=&endDate=&term=&prevVersionScreen=false'
INDEX_SHA='48c9e9d2f6049456ea4a0f27e383f60e57bb59b182d98ed6b6364970abfc6cb1'
TEXT='48f4e437621dbec0089da4a5347e9097637b8862975b3b1b47d9a9f83dfc6174'
ROSTER='ROLL CALL Attendee Name Title Status Libby Fay Mayor Present Maggie Huyck Trustee Present via Zoom Andrew Rice Trustee Present Micha Rosenoer Trustee Present Devin Rowe Trustee Present Cindie Swisher Trustee Present'
APPOINTMENT='MOTION NO. 03: MOVE TO APPOINT CHRIS STURM TO THE BUENA VISTA BOARD OF TRUSTEES.'
OATH='Town Clerk Paula Barnett administered the Oath of Office, and Trustee Sturm took his seat on the dais.'
CONTEXT='Flock Cameras Mayor Fay stated that the expenditure for a camera surveillance system has been allocated and approved in the 2025 Budget.'
SCOPE='Motion to direct Staff to evaluate partnership opportunities with Chaffee County with a Memorandum of Understanding (MOU) brought back to the Board for consideration.'
MOTION='MOTION NO. 04: MOVE TO DIRECT STAFF TO EVALUATE PARTNERSHIP OPPORTUNITIES WITH THE COUNTY WITH A MEMORANDUM OF UNDERSTANDING BROUGHT BACK TO THE BOARD FOR CONSIDERATION. RESULTS ROLL CALL CARRIED MOVER: Trustee Swisher SECONDER: Trustee Huyck AYES: Huyck, Rice, Sturm, Swisher NYES: Rosenoer, Rowe'
VOTES=[{'name':'Maggie Huyck','role':'Trustee','position':'Aye'}, {'name':'Andrew Rice','role':'Trustee','position':'Aye'}, {'name':'Chris Sturm','role':'Trustee','position':'Aye'}, {'name':'Cindie Swisher','role':'Trustee','position':'Aye'}, {'name':'Micha Rosenoer','role':'Trustee','position':'NYES (interpreted as no; unconfirmed)'}, {'name':'Devin Rowe','role':'Trustee','position':'NYES (interpreted as no; unconfirmed)'}]

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
        candidate.get('index_url')!=INDEX or candidate.get('index_sha256')!=INDEX_SHA or candidate.get('anchor_index')!=76 or
        current.get('url')!=URL or current.get('index_url')!=INDEX or 'buenavistaco.gov' not in r.discovery_hosts(data_dir,db,REF) or
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
    if (len(pages)!=9 or any(pages[n-1].count(value)!=1 for n,value in ((2,ROSTER),(4,APPOINTMENT),(4,OATH),(6,CONTEXT),(7,SCOPE),(7,MOTION))) or pages[3].index(APPOINTMENT)>pages[3].index(OATH) or pages[6].index(SCOPE)>pages[6].index(MOTION)):raise ValueError('roster, appointment, context or item vote differs')
    return {'schema_version':SCHEMA,'state':'available','research_record':RECORD,'jurisdiction_ref':REF,'jurisdiction':'Buena Vista',
      'meeting_date':'2025-01-28','body':'Board of Trustees',
      'motion':'Direct staff to evaluate a Chaffee County partnership concerning Flock cameras and return an MOU for consideration','outcome':'Carried; four AYES, two NYES', 'motion_details':'Trustee Swisher moved; Trustee Huyck seconded.',
      'votes':VOTES,'mayor':'Mayor Libby Fay was present; the motion’s roll-call table does not record a vote for her.', 'notice':'This motion directs staff to evaluate a partnership; it does not approve or establish an executed agreement, camera purchase or deployment. The source table prints NYES for two trustees; this is interpreted as no, but the source does not define the label.',
      'source':{'url':URL,'sha256':PDF,'text_sha256':TEXT,'roster_page':2,'appointment_page':4,'context_page':6,'motion_page':7,
        'attendance_locator':'OCR page 2','appointment_locator':'OCR page 4','motion_locator':'OCR page 7',
        'index_url':INDEX,'index_sha256':INDEX_SHA,'index_anchor':76,'index_receipt_sha256':index_receipt,
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
