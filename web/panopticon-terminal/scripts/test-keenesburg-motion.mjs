import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { validateKeenesburgMotion, validateKeenesburgResearchBinding } from '../keenesburg-motion-api.js';
const data = JSON.parse(readFileSync(new URL('./fixtures/keenesburg-motion-valid.json',import.meta.url)));
const report = {records: [{id: data.research_record, kind: 'meeting_document', historical: false, jurisdiction_ref: data.jurisdiction_ref, source_url: data.source.url, source_sha256: data.source.sha256, indexed_source_url: data.source.indexed_url, source_index_url: data.source.index_url, capture_adjustment: 'timestamp_omitted'}]};
test('valid source-bound motions and research linkage', () => {
  assert.equal(validateKeenesburgMotion(data).votes.length,7); validateKeenesburgResearchBinding(data,report);
  validateKeenesburgMotion({schema_version:data.schema_version,state:'not_available'});
});
test('tampered vote, scopes, links and receipts fail closed', () => {
  const attacks=[d=>d.votes[0].name='Other',d=>d.votes[6].position='No',d=>d.votes.push(d.votes[0]),
    d=>d.motion=d.separate_motion,d=>d.separate_motion='same vote',d=>d.source.motion_page=1,
    d=>d.source.indexed_url='https://attacker.example',d=>d.source.index_sha256='0'.repeat(64),
    d=>d.source.redirect_receipt_sha256='missing',d=>d.source.indexed_redirect_receipt_sha256='missing',d=>d.extra=true];
  for (const attack of attacks) { const copy=structuredClone(data);attack(copy);assert.throws(()=>validateKeenesburgMotion(copy)); }
});
test('missing, historical or mismatched research source cannot sustain motion', () => {
  const record=report.records.find(r=>r.id===data.research_record); validateKeenesburgResearchBinding(data,{records:[record]});
  assert.throws(()=>validateKeenesburgResearchBinding(data,{records:[]}));
  assert.throws(()=>validateKeenesburgResearchBinding(data,{records:[record,record]}));
  for (const key of ['kind','historical','jurisdiction_ref','source_url','source_sha256','indexed_source_url','source_index_url','capture_adjustment'])
    assert.throws(()=>validateKeenesburgResearchBinding(data,{records:[{...record,[key]:'changed'}]}));
});
