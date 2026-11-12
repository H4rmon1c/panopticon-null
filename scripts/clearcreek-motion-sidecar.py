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
SCHEMA='pnull-clearcreek-motion-v1'; REF='us-co-county-08019'; RECORD='co-document-7449fbbc2433c23d2da0f8ed'
URL='https://www.clearcreekcounty.us/AgendaCenter/ViewFile/Minutes/_12162025-901'
PDF='3029fdaa63306a107060a576a155347e2aee2e93f8aef4d9a347988275d2100c'
INDEX='https://www.clearcreekcounty.us/AgendaCenter/UpdateCategoryList?year=2025&catID=4&startDate=&endDate=&term=&prevVersionScreen=false'
INDEX_SHA='9d6cd24774e3c9d49e70f2cfc16e79a1ed21cfa93627ef2c268fd843fcc029e3'
TEXT='a24269216b3fc283d8f9b9b3e2eb6449f4678a95d6ae78ada406cd590036f7a8'
LINES={1: 'Meeting Minutes', 2: 'Clear Creek Board of County Commissioners', 3: '405 Argentine Street, Georgetown, CO 80444', 4: 'Regular Meeting', 5: 'December 16, 2025', 6: '8:30 a.m.', 7: 'ALL IN ATTENDANCE: Chair, George Marlin; Commissioner, Jodie Hartman Ball; Commissioner, Rebecca', 8: 'Lloyd; County Attorney, Peter A. Lichtman; County Manager, Colton Rohloff; Deputy Clerk to the Board, Amy', 9: 'Hendricks; Public Information Officer, Megan Hiler-Wilson; Director of Human Services, Sarah Cassano;', 38: 'BOARD OF COUNTY COMMISSIONERS ACTION SESSION', 39: '7. CONSENT AGENDA: The following Consent Agenda Item titles were read by Chair, George Marlin:', 40: 'A.', 41: 'Consideration of Approval of Warrant Register', 42: 'B.', 43: 'Consideration of Resolution R-25-89, A Resolution Designating the 2026 Clear Creek County Holiday', 44: 'Schedules for Clear Creek County Offices and the Clear Creek County Transfer Station', 45: '1 of 4', 46: '', 47: 'C.', 48: 'Consideration of Approval of Quote and Master Services Agreement with Axon Enterprises, Inc. for', 49: 'Body-Worn Cameras, In-Car Video Systems, TASER Devices, Digital Evidence Storage and Software', 50: 'Licensing', 51: 'D', 52: 'Consideration of Approval of the Clear Creek County and Prowers County Memorandum of', 53: 'Understanding for the Hotline County Connection Center', 54: 'E.', 55: 'Consideration of Approval of EMTS Funding Program Grant Attestation', 56: 'Motion: Moved to approve by Commissioner Hartman-Ball, seconded by Commissioner Lloyd', 57: 'Motion Result: Passed Unanimously'}
ATTENDANCE=[{'name':'George Marlin','role':'Chair','individual_vote':'Not individually recorded'},
 {'name':'Jodie Hartman Ball','role':'Commissioner','individual_vote':'Not individually recorded'},
 {'name':'Rebecca Lloyd','role':'Commissioner','individual_vote':'Not individually recorded'}]

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
        candidate.get('index_url')!=INDEX or candidate.get('index_sha256')!=INDEX_SHA or candidate.get('anchor_index')!=9 or
        current.get('url')!=URL or current.get('index_url')!=INDEX or 'clearcreekcounty.us' not in r.discovery_hosts(data_dir,db,REF) or
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
    return {'schema_version':SCHEMA,'state':'available','research_record':RECORD,'jurisdiction_ref':REF,'jurisdiction':'Clear Creek County',
      'meeting_date':'2025-12-16','body':'Board of County Commissioners',
      'motion':'Consent agenda including an Axon quote and master services agreement for body-worn cameras, in-car video, TASER devices, digital evidence storage and software licensing','outcome':'Consent agenda passed unanimously', 'block_motion':'Commissioner Hartman-Ball moved to approve the consent agenda; Commissioner Lloyd seconded.',
      'attendance':ATTENDANCE,'notice':'The recorded motion covers the entire consent agenda, including the Axon item. A separate item vote, individual roll-call votes, contract execution, delivery, deployment and office terms are not established by this record.',
      'source':{'url':URL,'sha256':PDF,'text_sha256':TEXT,'attendance_page':1,'motion_page':2,
        'attendance_locator':'extracted lines 1–9','motion_locator':'extracted lines 38–57',
        'index_url':INDEX,'index_sha256':INDEX_SHA,'index_anchor':9,'index_receipt_sha256':index_receipt,
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
