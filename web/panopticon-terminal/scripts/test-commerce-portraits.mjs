import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { validateResearch } from '../research-api.js';

const root = new URL('../../../', import.meta.url);
const proof = JSON.parse(await readFile(new URL('fixtures/atlas/portrait-sources/commerce-city-2022-report-proof.json', root), 'utf8'));
const report = JSON.parse(await readFile(new URL('../public/research-signals.json', import.meta.url), 'utf8'));
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const votes = (data) => data.records.filter((record) => record.jurisdiction_ref === 'us-co-place-0816495')
  .flatMap((record) => (record.actions ?? []).flatMap((motion) => motion.votes));

test('Commerce City report portraits bind to the exact portal voters and image streams', async () => {
  assert.equal(proof.source_page, 2);
  assert.equal(proof.portraits.length, 5);
  assert.equal(hash(await readFile(new URL(proof.page_fixture, root))), proof.page_fixture_sha256);
  validateResearch(structuredClone(report));
  for (const expected of proof.portraits) {
    const vote = votes(report).find((item) => item.person_id === expected.person_id && item.name === expected.name);
    assert.ok(vote?.portrait, `missing sourced portrait for ${expected.name}`);
    assert.equal(vote.portrait.embedded_image_index, expected.image_index);
    assert.equal(vote.portrait.source_document_sha256, proof.source_sha256);
    assert.equal(hash(await readFile(new URL(`../${vote.portrait.src.slice(2)}`, import.meta.url))), expected.sha256);
  }
  const changed = structuredClone(report);
  const first = votes(changed).find((vote) => vote.person_id === 402);
  const second = votes(changed).find((vote) => vote.person_id === 401);
  first.portrait = second.portrait;
  assert.throws(() => validateResearch(changed), /portrait/i);
});
