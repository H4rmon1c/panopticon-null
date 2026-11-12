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
SCHEMA='pnull-ouray-motion-v1'; REF='us-co-county-08091'; RECORD='co-document-a715fed2f776dc7376814eb0'
URL='https://ouraycountyco.gov/AgendaCenter/ViewFile/Minutes/_11192024-928'
PDF='b32eeec9d0467253693983f5f459675715a48f3d76be227239778a3f8a8bd10b'
INDEX='https://ouraycountyco.gov/AgendaCenter/UpdateCategoryList?year=2024&catID=1&startDate=&endDate=&term=&prevVersionScreen=false'
INDEX_SHA='8789592383ffc62b8d50c010ce212996f9a88e953d380e913aa5712e83a9bf60'
TEXT='c2ca5c366e9d6b31d249deedd9e165401570709c777f967f7bf27c0438553a40'
LINES={1:'The Board of County Commissioners met in a regular session on November 19, 2024. Those present for the',
 2:'session were Michelle Nauer, Chair; Lynn Padgett, Vice-Chair; Jake Niece, Commissioner Member; Connie Hunt,',
 127:'6. Request for approval and authorization of Chairs signature on Quotation with Axon',
 128:'Enterprise, Inc., for software and services for tasers and camera equipment and on its Fiscal',
 129:'Impact Form:',
 130:'M/S/P Motion was made by Jake Niece and seconded by Lynn Padgett approve and authorize the Chairs',
 131:'signature on Quotation with Axon Enterprise, Inc., for software and services for tasers and camera',
 132:'equipment and on its Fiscal Impact Form. Motion passed unanimously',
 133:'7. Request for approval and authorization of Chairs signature on Budgetary Transfer Form:'}
ATTENDANCE=[{'name':'Michelle Nauer','role':'Chair','individual_vote':'Not individually recorded'},
 {'name':'Lynn Padgett','role':'Vice-Chair','individual_vote':'Not individually recorded'},
 {'name':'Jake Niece','role':'Commissioner','individual_vote':'Not individually recorded'}]

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
        candidate.get('index_url')!=INDEX or candidate.get('index_sha256')!=INDEX_SHA or candidate.get('anchor_index')!=39 or
        current.get('url')!=URL or current.get('index_url')!=INDEX or 'ouraycountyco.gov' not in r.discovery_hosts(data_dir,db,REF) or
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
    return {'schema_version':SCHEMA,'state':'available','research_record':RECORD,'jurisdiction_ref':REF,'jurisdiction':'Ouray County',
      'meeting_date':'2024-11-19','body':'Board of County Commissioners',
      'motion':'Approve and authorize the chair’s signature on an Axon quotation and fiscal impact form for taser and camera software and services','outcome':'Motion passed unanimously',
      'attendance':ATTENDANCE,'notice':'The motion authorized signatures. Individual votes, actual signatures, contract execution, delivery, deployment and office terms are not established by this record.',
      'source':{'url':URL,'sha256':PDF,'text_sha256':TEXT,'attendance_page':1,'motion_page':2,
        'attendance_locator':'extracted lines 1–2','motion_locator':'extracted lines 127–132',
        'index_url':INDEX,'index_sha256':INDEX_SHA,'index_anchor':39,'index_receipt_sha256':index_receipt,
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
