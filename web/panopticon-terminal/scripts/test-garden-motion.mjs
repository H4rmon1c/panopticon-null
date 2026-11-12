import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { validateGardenMotion, validateGardenResearchBinding } from '../garden-motion-api.js';
const valid = JSON.parse(readFileSync(new URL('./fixtures/garden-motion-valid.json', import.meta.url)));
test('source-bound motion and separately cited consent adoption validate', () => {
  assert.equal(validateGardenMotion(valid).votes.length, 6);
  assert.equal(valid.minutes_adoption.yes_count, 7);
  assert.equal(validateGardenMotion({ ...valid, minutes_adoption: null }).minutes_adoption, null);
  assert.equal(validateGardenMotion({ schema_version: valid.schema_version, state: 'not_available' }).state, 'not_available');
});
test('changed vote, identity, source or consent scope fail closed', () => {
  const attacks = [
    d => d.votes[0].name = 'Other', d => d.votes[0].position = 'No', d => d.votes.push(d.votes[0]),
    d => d.attendance.status = 'Yes', d => d.source.motion_page = 3, d => d.source.sha256 = '0'.repeat(64),
    d => d.source.url = 'https://evil.example/', d => d.source.receipt_sha256 = 'missing',
    d => d.minutes_adoption.yes_count = 6, d => d.minutes_adoption.scope = 'Axon agreement',
    d => d.minutes_adoption.source.meeting_page = 2, d => d.minutes_adoption.source.vote_page = 1,
    d => d.minutes_adoption.source.index_anchor = 90, d => d.minutes_adoption.source.url += '?fake=1',
    d => d.extra = 'unexpected'
  ];
  for (const attack of attacks) { const copy = structuredClone(valid); attack(copy); assert.throws(() => validateGardenMotion(copy)); }
});

test('motion card must match the published research source', () => {
  const record = { id: valid.research_record, kind: 'meeting_document', historical: false, jurisdiction_ref: valid.jurisdiction_ref, source_url: valid.source.url, source_sha256: valid.source.sha256 };
  validateGardenResearchBinding(valid, { records: [record] });
  assert.throws(() => validateGardenResearchBinding(valid, { records: [] }));
  assert.throws(() => validateGardenResearchBinding(valid, { records: [record, record] }));
  for (const key of ['id', 'kind', 'historical', 'jurisdiction_ref', 'source_url', 'source_sha256']) {
    assert.throws(() => validateGardenResearchBinding(valid, { records: [{ ...record, [key]: 'changed' }] }));
  }
  validateGardenResearchBinding({ state: 'not_available' }, { records: [] });
});
