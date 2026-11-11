import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { validateResearch } from '../research-api.js';

const root = new URL('../../../', import.meta.url);
const proof = JSON.parse(await readFile(new URL('fixtures/atlas/portrait-sources/arapahoe-2022-budget-proof.json', root), 'utf8'));
const proof2024 = JSON.parse(await readFile(new URL('fixtures/atlas/portrait-sources/arapahoe-2024-budget-proof.json', root), 'utf8'));
const report = JSON.parse(await readFile(new URL('../public/research-signals.json', import.meta.url), 'utf8'));
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');

test('the county budget portraits attach only to the exact recorded portal identities', async () => {
  assert.equal(proof.source_page, 7);
  assert.equal(hash(await readFile(new URL(proof.page_fixture, root))), proof.page_fixture_sha256);
  assert.equal(proof.portraits.length, 5);
  validateResearch(structuredClone(report));
  for (const expected of proof.portraits) {
    const vote = report.records.filter((record) => record.jurisdiction_ref === 'us-co-county-08005')
      .flatMap((record) => (record.actions ?? []).flatMap((motion) => motion.votes))
      .find((item) => item.person_id === expected.person_id && item.name === expected.name);
    assert.ok(vote, `missing source-recorded voter ${expected.name}`);
    const portrait = vote.portrait;
    assert.equal(portrait.source_kind, 'embedded_pdf');
    assert.equal(portrait.embedded_image_index, expected.image_index);
    assert.equal(portrait.source_document_sha256, proof.source_sha256);
    assert.equal(hash(await readFile(new URL(`../${portrait.src.slice(2)}`, import.meta.url))), expected.sha256);
  }
  const changed = structuredClone(report);
  const votes = changed.records.filter((record) => record.jurisdiction_ref === 'us-co-county-08005')
    .flatMap((record) => (record.actions ?? []).flatMap((motion) => motion.votes));
  const first = votes.find((vote) => vote.person_id === 513);
  const second = votes.find((vote) => vote.person_id === 225);
  first.portrait = second.portrait;
  assert.throws(() => validateResearch(changed), /portrait/i);
  const changedCaption = structuredClone(report);
  const changedVote = changedCaption.records.flatMap((record) => (record.actions ?? [])
    .flatMap((motion) => motion.votes)).find((vote) => vote.portrait?.source_kind === 'embedded_pdf');
  changedVote.portrait.caption = 'Incorrect source label';
  assert.throws(() => validateResearch(changedCaption), /portrait/i);
});

test('2024 portraits bind both voters and preserve the separate name-identity source', async () => {
  assert.equal(proof2024.source_page, 61);
  assert.equal(hash(await readFile(new URL(proof2024.page_fixture, root))), proof2024.page_fixture_sha256);
  validateResearch(structuredClone(report));
  for (const expected of proof2024.portraits) {
    const vote = report.records.filter((record) => record.jurisdiction_ref === 'us-co-county-08005')
      .flatMap((record) => (record.actions ?? []).flatMap((motion) => motion.votes))
      .find((item) => item.person_id === expected.person_id && item.name === expected.name);
    assert.ok(vote?.portrait, `missing sourced portrait for ${expected.name}`);
    assert.equal(vote.portrait.embedded_image_index, expected.image_index);
    assert.equal(vote.portrait.source_document_sha256, proof2024.source_sha256);
    assert.equal(hash(await readFile(new URL(`../${vote.portrait.src.slice(2)}`, import.meta.url))), expected.sha256);
    assert.equal(vote.portrait.identity_source_url, expected.person_id === 572 ? proof2024.alias_source_url : undefined);
  }
  const changed = structuredClone(report);
  const vote = changed.records.flatMap((record) => (record.actions ?? []).flatMap((motion) => motion.votes))
    .find((item) => item.name === 'Jessica Campbell' && item.person_id === 572);
  vote.portrait.identity_source_url = 'https://attacker.example/';
  assert.throws(() => validateResearch(changed), /portrait/i);
});
