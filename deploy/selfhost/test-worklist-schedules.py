#!/usr/bin/env python3
"""Selecting a schedule must never imply reviewed or executed discovery."""
import importlib.util
import json
from pathlib import Path
import tempfile

ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('worklist',ROOT/'web/panopticon-terminal/scripts/build-colorado-worklist.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)

def count(rows,key): return sum(bool(row[key]) for row in rows)

def check(rows):
    assert len(rows)==4111
    assert all(row['source_review_state']=='not_checked' and row['search_execution_state']=='not_measured' for row in rows)
    assert count(rows,'configured_calendar_ids')==7 and count(rows,'document_index_ids')==5
    assert sum(row['stage']=='not_started' for row in rows)==4099

base=module.build_rows();check(base)
assert count(base,'scheduled_calendar_ids')==0 and count(base,'scheduled_document_index_ids')==0
small=module.build_rows(ROOT/'deploy/selfhost/collection-small-town-sources.tsv');check(small)
assert count(small,'scheduled_calendar_ids')==0 and count(small,'scheduled_document_index_ids')==5
assert sum(row['discovery_schedule_state']=='selected_in_matrix' for row in small)==5
broad=module.build_rows(ROOT/'deploy/selfhost/collection-sources.tsv');check(broad)
assert count(broad,'scheduled_calendar_ids')==7 and count(broad,'scheduled_document_index_ids')==0
with tempfile.TemporaryDirectory(dir=ROOT/'deploy/selfhost',prefix='.worklist-test-') as temporary:
    combined=Path(temporary)/'combined.tsv'
    combined.write_text((ROOT/'deploy/selfhost/collection-sources.tsv').read_text()+(ROOT/'deploy/selfhost/collection-small-town-sources.tsv').read_text())
    # Regression: a configured index becoming scheduled used to make the
    # offline worklist throw "invalid unscheduled town index binding".
    previous=module.MATRIX;module.MATRIX=combined
    try:
        mixed=module.build_rows(combined);check(mixed)
        assert count(mixed,'scheduled_calendar_ids')==7 and count(mixed,'scheduled_document_index_ids')==5
    finally: module.MATRIX=previous
with tempfile.TemporaryDirectory(prefix='panopticon-directory-test-') as temporary:
    directory=Path(temporary)/'municipalities.json'
    original=json.loads(module.MUNICIPAL_DIRECTORY.read_text())
    valid={row['id']:row for row in json.loads(module.GEOGRAPHY.read_text())['jurisdictions']}
    previous=module.MUNICIPAL_DIRECTORY;module.MUNICIPAL_DIRECTORY=directory
    try:
        for mutation in ('digest','name','url','page','source'):
            data=json.loads(json.dumps(original))
            item=next(row for row in data['municipalities'] if row['listing_name']=='Iliff')
            if mutation=='digest': data['supplemental_sources']['cml_directory_2026']['sha256']='0'*64
            if mutation=='name': item['listing_name']='Another Town'
            if mutation=='url': item['website_url']='https://attacker.example/'
            if mutation=='page': item['source_locator']='2026 municipal directory, PDF page 165'
            if mutation=='source': item['website_source']='unknown'
            directory.write_text(json.dumps(data))
            try: module.municipal_website_leads(valid)
            except ValueError: pass
            else: raise AssertionError(f'accepted altered directory {mutation}')
    finally: module.MUNICIPAL_DIRECTORY=previous
print('Worklist distinguishes configured profiles, selected schedule, unknown review state and unmeasured searches.')
