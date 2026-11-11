import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { validateResearch } from '../research-api.js';

const root = new URL('../../../', import.meta.url);
const proof = JSON.parse(await readFile(new URL('fixtures/atlas/portrait-sources/littleton-2023-report-proof.json', root), 'utf8'));
const later = JSON.parse(await readFile(new URL('fixtures/atlas/portrait-sources/littleton-2024-report-proof.json', root), 'utf8'));
const report = JSON.parse(await readFile(new URL('../public/research-signals.json', import.meta.url), 'utf8'));
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const votesIn = (value) => value.records.filter((record) => record.jurisdiction_ref === 'us-co-place-0845255')
  .flatMap((record) => (record.actions ?? []).flatMap((motion) => motion.votes));

test('city portraits attach to the exact Littleton voter and preserve source and conversion metadata', async () => {
  assert.equal(proof.source_page, 3);
  assert.equal(proof.portraits.length, 7);
  assert.equal(hash(await readFile(new URL(proof.page_fixture, root))), proof.page_fixture_sha256);
  validateResearch(structuredClone(report));
  for (const expected of proof.portraits) {
    const vote = votesIn(report).find((item) => item.person_id === expected.person_id && item.name === expected.name);
    assert.ok(vote, `missing source-recorded voter ${expected.name}`);
    assert.equal(vote.portrait.embedded_image_index, expected.image_index);
    assert.equal(vote.portrait.embedded_original_sha256, expected.original_sha256);
    assert.equal(vote.portrait.image_transform, 'lossless_png_from_jpeg2000');
    assert.equal(vote.portrait.source_document_sha256, proof.source_sha256);
    assert.equal(hash(await readFile(new URL(`../${vote.portrait.src.slice(2)}`, import.meta.url))), expected.sha256);
  }
});

test('swapped identities and misleading transformation details are rejected', () => {
  const swapped = structuredClone(report);
  const votes = votesIn(swapped);
  votes.find((vote) => vote.person_id === 333).portrait = votes.find((vote) => vote.person_id === 434).portrait;
  assert.throws(() => validateResearch(swapped), /portrait/i);
  const changed = structuredClone(report);
  votesIn(changed).find((vote) => vote.person_id === 333).portrait.image_transform = 'original';
  assert.throws(() => validateResearch(changed), /portrait/i);
  const changedOriginal = structuredClone(report);
  votesIn(changedOriginal).find((vote) => vote.person_id === 333).portrait.embedded_original_sha256 = '0'.repeat(64);
  assert.throws(() => validateResearch(changedOriginal), /portrait/i);
});

test('later city report supplies the two remaining exact voter portraits', async () => {
  assert.equal(later.portraits.length, 2);
  assert.equal(hash(await readFile(new URL(later.page_fixture, root))), later.page_fixture_sha256);
  validateResearch(structuredClone(report));
  for (const expected of later.portraits) {
    const vote = votesIn(report).find((item) => item.person_id === expected.person_id && item.name === expected.name);
    assert.ok(vote, `missing source-recorded voter ${expected.name}`);
    assert.equal(vote.portrait.embedded_image_index, expected.image_index);
    assert.equal(vote.portrait.source_document_sha256, later.source_sha256);
    assert.equal(vote.portrait.image_transform, undefined);
    assert.equal(hash(await readFile(new URL(`../${vote.portrait.src.slice(2)}`, import.meta.url))), expected.sha256);
  }
  const swapped = structuredClone(report);
  const votes = votesIn(swapped);
  votes.find((vote) => vote.person_id === 503).portrait = votes.find((vote) => vote.person_id === 502).portrait;
  assert.throws(() => validateResearch(swapped), /portrait/i);
});
