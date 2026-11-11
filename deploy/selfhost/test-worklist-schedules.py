#!/usr/bin/env python3
"""Selecting a schedule must never imply reviewed or executed discovery."""
import importlib.util
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
print('Worklist distinguishes configured profiles, selected schedule, unknown review state and unmeasured searches.')
