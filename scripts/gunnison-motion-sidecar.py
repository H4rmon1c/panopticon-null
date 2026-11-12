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
SCHEMA='pnull-gunnison-motion-v1'; REF='us-co-county-08051'; RECORD='co-document-90325481a4958003979ad7f9'
URL='https://gunnisoncounty.org/AgendaCenter/ViewFile/Minutes/_09152026-1205'
PDF='79d3616ac6091c551f2be7d8f548c02becc0fbfff243e22ae664f231943a786f'
INDEX='https://gunnisoncounty.org/AgendaCenter'
INDEX_SHA='0375ccd729eedb424009815981ad24d39c86b0530a2b1f4eb6f8a4809a6622c1'
TEXT='5480cfe5639a8a028f556569c93b0066eb059bbfbbdb58559d83f7da336a3b37'
LINES={1:'September 15, 2026',
 2:'GUNNISON COUNTY BOARD OF COUNTY COMMISSIONERS',
 3:'REGULAR MEETING MINUTES',
 4:'September 15, 2026',
 5:'The September 15, 2026 meeting was held in the Board of County Commissioners’ meeting room located',
 6:'at 200 E. Virginia Avenue, Gunnison, Colorado. Present, either in person or via Zoom, were:',
 7:'Laura Puckett Daniels, Chairperson',
 8:'Elizabeth Smith, Vice-Chairperson',
 9:'Jonathan Houck, Commissioner',
 112:'GUNNISON COUNTY BOARD OF COUNTY COMMISSIONERS REGULAR MEETING:',
 113:'CALL TO ORDER: Commissioner Puckett Daniels called the meeting to order at 10:36 am.',
 127:'CONSENT AGENDA: Moved by Commissioner Houck, seconded by Commissioner Smith to approve the',
 128:'consent agenda, with the exception of item 10, which we will discuss separately. Motion carried',
 129:'unanimously.',
 147:'10. PULLED FOR FUTHER DISCUSSION: Written Notice of Nonrenewal; Flock Group, Inc',
 148:'11. Assumption of Fire Duty Control; Gold Mountain and Elk Fire; Sheriff’s Office',
 149:'PULLED FOR FUTHER DISCUSSION:',
 150:'10. Written Notice of Nonrenewal; Flock Group, Inc – Commissioner Smith relayed her concerns about',
 151:'data sharing even after the termination of the contract. CA Hoyt noted that the contract requires',
 152:'both sides to return proprietary information and that the cameras will be removed in a reasonable',
 153:'timeframe. Moved by Commissioner Smith, seconded by to Commissioner Houck to approve item',
 154:'number 10, the written notice of non-renewal with the Flock Group, Inc and authorize the Chair’s',
 155:'signature. Commissioner Puckett Daniels noted that this was the direction of the Sheriff to cancel',
 156:'this agreement. Motion carried unanimously.'}
ATTENDANCE=[{'name':'Laura Puckett Daniels','role':'Chairperson','individual_vote':'Not individually recorded'},
 {'name':'Elizabeth Smith','role':'Vice-Chairperson','individual_vote':'Not individually recorded'},
 {'name':'Jonathan Houck','role':'Commissioner','individual_vote':'Not individually recorded'}]

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
        candidate.get('index_url')!=INDEX or candidate.get('index_sha256')!=INDEX_SHA or candidate.get('anchor_index')!=206 or
        current.get('url')!=URL or current.get('index_url') not in (INDEX,INDEX+'#') or 'gunnisoncounty.org' not in r.discovery_hosts(data_dir,db,REF) or
        not r.receipt_valid(receipt,URL,PDF)):
        raise ValueError('source identity differs')
    raw=(json.dumps(receipt,sort_keys=True,separators=(',',':'))+'\n').encode();receipt_sha=r.sha(raw)
    if r.read_blob(data_dir/'observations'/(receipt_sha+'.json'),65536)!=raw:raise ValueError('document receipt differs')
    index_receipt=receipt_digest(data_dir,INDEX,INDEX_SHA)
    current_index_receipt=receipt_digest(data_dir,current['index_url'],current['index_sha256'])
    r.checked_document_candidate(data_dir,URL,candidate);r.checked_document_candidate(data_dir,URL,current)
    if not r.source_blob(data_dir,PDF,20*1024*1024).startswith(b'%PDF-'):raise ValueError('source is not a PDF')
    text=r.read_blob(data_dir/'extracted'/TEXT,5*1024*1024)
    if r.sha(text)!=TEXT:raise ValueError('extracted digest differs')
    lines=text.decode('utf8').splitlines()
    if any(n>len(lines) or lines[n-1]!=v for n,v in LINES.items()):raise ValueError('meeting, motion or attendance differs')
    return {'schema_version':SCHEMA,'state':'available','research_record':RECORD,'jurisdiction_ref':REF,'jurisdiction':'Gunnison County',
      'meeting_date':'2026-09-15','body':'Board of County Commissioners',
      'motion':'Approve a written notice of nonrenewal with Flock Group and authorize the chair’s signature','outcome':'Motion passed unanimously','motion_details':'Commissioner Smith moved to approve the notice and authorize the chair’s signature; Commissioner Houck seconded.',
      'attendance':ATTENDANCE,'notice':'Item 10 was removed from the consent agenda and decided separately. Individual votes, a signed or delivered notice, actual camera removal, camera type, original contract approval and office terms are not established by this record.',
      'source':{'url':URL,'sha256':PDF,'text_sha256':TEXT,'attendance_page':1,'motion_page':3,
        'attendance_locator':'extracted lines 1–9','motion_locator':'extracted lines 127–129 and 147–156',
        'index_url':INDEX,'index_sha256':INDEX_SHA,'index_anchor':206,'index_receipt_sha256':index_receipt,
        'current_index_url':current['index_url'],'current_index_sha256':current['index_sha256'],'current_index_anchor':current['anchor_index'],
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
