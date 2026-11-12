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
SCHEMA='pnull-columbine-motion-v1'; REF='us-co-place-0816385'; RECORD='co-document-cf6d9e8394298e3b40d75489'
URL='https://columbinevalley.org/wp-content/uploads/2025/06/05-20-25-Board-of-Trustee-Minutes.pdf'
PDF='4616214c06fe1a10fc859c987a4ff5ec83091fb09aae3e23c6c5bacc85841c48'
INDEX='https://columbinevalley.org/board-trustee-agendas-minutes/'
INDEX_SHA='cf4629ca406d3ffc660c188a57a19c63f3051eabdd087177de2e99b13b8a0896'
TEXT='67f5f0e649eea711ad08794b89a2ae0e75dc1008beba4e11e7c88c4d8f85df7b'
LINES={1:'TOWN OF COLUMBINE VALLEY',2:'BOARD OF TRUSTEES',3:'Minutes',4:'May 20, 2025',
 5:'Mayor Pro Tem Dotson called the Regular Meeting of the Trustees to order at 6:30 p.m., in the',
 6:'Conference Room at the Town Hall at 2 Middlefield Road, Columbine Valley, Colorado. Roll call',
 7:'found the following present:',8:'Trustees:',9:'Also present:',
 10:'Bill Dotson, Mike Giesen, Dave Huelskamp, Ed Icenogle, Betsy',
 11:'McCain, and Al Timothy*',14:'*Participated virtually',
 45:'License Agreement with the City of Littleton: Mr. McCrumb presented the agreement',
 46:'which would allow the Town to place a Flock camera in Littleton right-of-way. The Trustees',
 47:'asked clarifying questions.',48:'ACTION: upon a motion by Trustee Icenogle and a second by Trustee Giesen,',
 49:'the Board of Trustees unanimously approved the agreement.',
 50:'Trustee Bill #1-2025 – Model Traffic Code (1 st Reading): Mr. Schiller presented the'}
ATTENDANCE=[{'name':'Bill Dotson','role':'Mayor Pro Tem','individual_vote':'Not individually recorded'},
 {'name':'Mike Giesen','role':'Trustee','individual_vote':'Not individually recorded'},
 {'name':'Dave Huelskamp','role':'Trustee','individual_vote':'Not individually recorded'},
 {'name':'Ed Icenogle','role':'Trustee','individual_vote':'Not individually recorded'},
 {'name':'Betsy McCain','role':'Trustee','individual_vote':'Not individually recorded'},
 {'name':'Al Timothy','role':'Trustee (participated virtually)','individual_vote':'Not individually recorded'}]

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
        candidate.get('index_url')!=INDEX or candidate.get('index_sha256')!=INDEX_SHA or candidate.get('anchor_index')!=135 or
        current.get('url')!=URL or current.get('index_url')!=INDEX or 'columbinevalley.org' not in r.discovery_hosts(data_dir,db,REF) or
        not r.receipt_valid(receipt,URL,PDF)):
        raise ValueError('source identity differs')
    raw=(json.dumps(receipt,sort_keys=True,separators=(',',':'))+'\n').encode();receipt_sha=r.sha(raw)
    if r.read_blob(data_dir/'observations'/(receipt_sha+'.json'),65536)!=raw:raise ValueError('document receipt differs')
    index_receipt=receipt_digest(data_dir,INDEX,INDEX_SHA)
    current_index_receipt=receipt_digest(data_dir,INDEX,current['index_sha256'])
    r.checked_document_candidate(data_dir,URL,candidate);r.checked_document_candidate(data_dir,URL,current)
    links,_=r.page_links(r.source_blob(data_dir,INDEX_SHA),INDEX)
    if ' '.join(links[135][1].split())!='05-10-25 Board of Trustees Minutes':raise ValueError('archived index date label differs')
    if not r.source_blob(data_dir,PDF,20*1024*1024).startswith(b'%PDF-'):raise ValueError('source is not a PDF')
    text=r.read_blob(data_dir/'extracted'/TEXT,5*1024*1024)
    if r.sha(text)!=TEXT:raise ValueError('extracted digest differs')
    lines=text.decode('utf8').splitlines()
    if any(n>len(lines) or lines[n-1]!=v for n,v in LINES.items()):raise ValueError('meeting, motion or attendance differs')
    return {'schema_version':SCHEMA,'state':'available','research_record':RECORD,'jurisdiction_ref':REF,'jurisdiction':'Columbine Valley',
      'meeting_date':'2025-05-20','body':'Board of Trustees',
      'motion':'Approve a Littleton license agreement described as allowing the town to place a Flock camera in Littleton right-of-way','outcome':'Motion passed unanimously', 'motion_details':'Trustee Icenogle moved to approve the agreement; Trustee Giesen seconded.',
      'attendance':ATTENDANCE,'notice':'The archived town index labeled this file May 10; the minutes themselves are dated May 20. Individual votes, the executed agreement, camera purchase, installation, camera type, current use and office terms are not established by this record.',
      'source':{'url':URL,'sha256':PDF,'text_sha256':TEXT,'attendance_page':1,'motion_page':2,
        'attendance_locator':'extracted lines 1–14','motion_locator':'extracted lines 45–49',
        'index_url':INDEX,'index_sha256':INDEX_SHA,'index_anchor':135,'index_receipt_sha256':index_receipt,
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
