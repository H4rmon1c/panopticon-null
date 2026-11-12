#!/usr/bin/env python3
"""Materialize one source-bound, preliminary Cortez final-reading vote."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import sqlite3

ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('research',ROOT/'scripts/colorado-research.py')
r=importlib.util.module_from_spec(spec);spec.loader.exec_module(r)
SCHEMA='pnull-cortez-motion-v1';REF='us-co-place-0817375';RECORD='co-document-86c05ac1faae3595c83d3344'
URL='https://www.cortezco.gov/AgendaCenter/ViewFile/Minutes/_03232021-768'
PDF='9a9a750885280e7cad5226d573ace94713cfa50d547e26b55eba96aebb72d67a'
INDEX='https://www.cortezco.gov/AgendaCenter/UpdateCategoryList?year=2021&catID=3&startDate=&endDate=&term=&prevVersionScreen=false'
INDEX_SHA='305f5dfe53f80c0fe37951fa2224b045affc532e77030e97f073c06e10c5c60e'
TEXT_SHA='f1c88cdb6bfc865458f9e627f3c1c175efce24deefaf82fa99dd54bf0b32aa32'
LINES={1:'CITY COUNCIL',2:'REGULAR MEETING',3:'TUESDAY, MARCH 23, 2021',
 6:'Councilmembers present in the Council Chambers included Mayor Mike Lavey, Robert Dobry,',
 7:'Amy Huckins, Mayor Pro-tem Rachel Medina, David Rainey, and Arlina Yazzie. Councilmember',
 8:'Orly Lucero participated by ZOOM. Staff members present by ZOOM were Airport Manager',
 70:'4.',71:'PUBLIC HEARINGS',
 72:'a. Ordinance No. 1286, Series 2021. City Attorney Green stated that Ordinance No. 1286, Series',
 73:'2021, is presented on second reading and authorizes the purchase of items from Axon Enterprise,',
 74:'Inc. for the Police Department. He stated that the items that are being purchased include new',
 75:'tasers, body cameras, and equipment for an interview room with upgraded storage of digital',
 76:'materials. Mayor Lavey opened the public hearing; however, no one spoke and the hearing was',
 77:'closed.',78:'Councilmember Dobry moved that Council approve on final reading Ordinance No. 1286, Series',
 79:'2021, approving the purchase of items from Axon Enterprise, Inc. Councilmember Rainey',
 80:'seconded the motion, and the vote was as follows:',81:'Dobry',82:'Yes',83:'5.',84:'Huckins',85:'Yes',
 86:'Lavey',87:'Yes',88:'Lucero',89:'Yes',90:'Medina',91:'Yes',92:'Rainey',93:'Yes',94:'Yazzie',95:'Yes',96:'UNFINISHED BUSINESS – None.'}
VOTES=[{'name':n,'role':role,'table_label':label,'position':'Yes'} for n,role,label in [
 ('Robert Dobry','Councilmember','Dobry'),('Amy Huckins','Councilmember','Huckins'),('Mike Lavey','Mayor','Lavey'),
 ('Orly Lucero','Councilmember','Lucero'),('Rachel Medina','Mayor Pro Tem','Medina'),
 ('David Rainey','Councilmember','Rainey'),('Arlina Yazzie','Councilmember','Yazzie')]]


def receipt_digest(data_dir,url,digest):
    found=[]
    for path in (data_dir/'observations').iterdir():
        try:
            raw=r.read_blob(path,65536);item=json.loads(raw)
            if path.name==r.sha(raw)+'.json' and r.receipt_valid(item,url,digest):found.append(path.stem)
        except (OSError,ValueError,TypeError,KeyError,json.JSONDecodeError):continue
    if not found:raise ValueError('source receipt unavailable')
    return min(found)


def materialize(data_dir,db):
    row=db.execute('SELECT state,candidate_json,result_json FROM document_candidates WHERE jurisdiction_ref=? AND url=?',(REF,URL)).fetchone()
    if not row or row[0]!='matched':raise ValueError('current source unavailable')
    result=json.loads(row[2]);candidate=result.get('candidate',json.loads(row[1]));receipt=result['receipt']
    if (result.get('document_sha256')!=PDF or result.get('extracted_sha256')!=TEXT_SHA or candidate.get('url')!=URL or
        candidate.get('index_url')!=INDEX or candidate.get('index_sha256')!=INDEX_SHA or candidate.get('anchor_index')!=69 or
        'cortezco.gov' not in r.discovery_hosts(data_dir,db,REF) or not r.receipt_valid(receipt,URL,PDF)):
        raise ValueError('source identity differs')
    raw=(json.dumps(receipt,sort_keys=True,separators=(',',':'))+'\n').encode();receipt_sha=r.sha(raw)
    if r.read_blob(data_dir/'observations'/(receipt_sha+'.json'),65536)!=raw:raise ValueError('source receipt differs')
    index_receipt=receipt_digest(data_dir,INDEX,INDEX_SHA)
    r.checked_document_candidate(data_dir,URL,candidate)
    if not r.source_blob(data_dir,PDF,20*1024*1024).startswith(b'%PDF-'):raise ValueError('source is not a PDF')
    text=r.read_blob(data_dir/'extracted'/TEXT_SHA,5*1024*1024)
    if r.sha(text)!=TEXT_SHA:raise ValueError('extracted digest differs')
    lines=text.decode('utf8').splitlines()
    if any(n>len(lines) or lines[n-1]!=v for n,v in LINES.items()):raise ValueError('roster, motion or vote differs')
    return {'schema_version':SCHEMA,'state':'available','research_record':RECORD,'jurisdiction_ref':REF,'jurisdiction':'Cortez',
       'meeting_date':'2021-03-23','body':'City Council','motion':'Final reading of Ordinance 1286 approving purchase of items from Axon Enterprise, Inc.',
       'scope':'Item-specific final-reading vote; separate from the consent agenda.','outcome':'Seven yes votes','votes':VOTES,
       'notice':'Minutes describe tasers, body cameras and interview-room equipment. Executed contract, delivery, deployment and office terms are not established by this record.',
       'source':{'url':URL,'sha256':PDF,'text_sha256':TEXT_SHA,'roster_page':1,'vote_page':2,
         'roster_locator':'extracted lines 1–8','motion_locator':'extracted lines 70–80','vote_locator':'extracted lines 81–95; visual table on page 2',
         'index_url':INDEX,'index_sha256':INDEX_SHA,'index_anchor':69,'receipt_sha256':receipt_sha,'index_receipt_sha256':index_receipt,
         'captured_at':receipt['retrieved_at']}}


def require_research_binding(snapshot):
    records=[x for x in snapshot['records'] if x.get('id')==RECORD]
    if (len(records)!=1 or records[0].get('kind')!='meeting_document' or records[0].get('historical') is not False or
        records[0].get('jurisdiction_ref')!=REF or records[0].get('source_url')!=URL or records[0].get('source_sha256')!=PDF):
        raise ValueError('research source unavailable or changed')


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--data-dir',required=True,type=Path);p.add_argument('--output',required=True,type=Path);p.add_argument('--research-snapshot',type=Path);a=p.parse_args()
    if not a.data_dir.is_absolute() or not a.output.is_absolute():p.error('absolute paths required')
    try:
        with sqlite3.connect((a.data_dir/'research.sqlite3').as_uri()+'?mode=ro',uri=True) as db:
            db.execute('BEGIN');result=materialize(a.data_dir,db);db.rollback()
        if a.research_snapshot is not None:
            if not a.research_snapshot.is_absolute():raise ValueError('snapshot path must be absolute')
            require_research_binding(json.loads(r.read_blob(a.research_snapshot,8*1024*1024)))
    except (OSError,ValueError,TypeError,KeyError,sqlite3.Error,json.JSONDecodeError):result={'schema_version':SCHEMA,'state':'not_available'}
    a.output.parent.mkdir(parents=True,exist_ok=True);temporary=a.output.with_name('.'+a.output.name+f'.{os.getpid()}.tmp')
    fd=os.open(temporary,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o644)
    try:
        with os.fdopen(fd,'w') as f:json.dump(result,f,separators=(',',':'));f.write('\n');f.flush();os.fsync(f.fileno())
        os.replace(temporary,a.output)
    finally:
        if temporary.exists():temporary.unlink()
if __name__=='__main__':main()
