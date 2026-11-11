import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { assertEmbeddedPortraitMatches } from './validate-embedded-portrait.mjs';

const base = new URL('../../../fixtures/atlas/portrait-sources/', import.meta.url);
const load = async (name) => JSON.parse(await readFile(new URL(name, base), 'utf8'));
const proofs = Object.fromEntries(await Promise.all(Object.entries({
  proof: 'arapahoe-2022-budget-proof.json',
  arapahoe2024: 'arapahoe-2024-budget-proof.json',
  littleton: 'littleton-2023-report-proof.json',
  later: 'littleton-2024-report-proof.json',
  commerce: 'commerce-city-2022-report-proof.json',
  denver2019: 'denver-2019-budget-proof.json',
  denver2020: 'denver-2020-budget-proof.json',
}).map(async ([key, file]) => [key, await load(file)])));

const denver = { jurisdiction_ref: 'us-co-place-0820000' };
function voteFor(proof, person) {
  return {
    name: person.name, person_id: person.person_id,
    portrait: {
      source_document_sha256: proof.source_sha256, source_url: proof.source_url,
      source_page: proof.source_page, embedded_image_index: person.image_index,
      embedded_original_sha256: person.original_sha256, sha256: person.sha256,
    },
  };
}

test('publication validator accepts both pinned Denver budget sources', () => {
  for (const person of proofs.denver2019.portraits) {
    assert.doesNotThrow(() => assertEmbeddedPortraitMatches(denver, voteFor(proofs.denver2019, person), proofs));
  }
  assert.doesNotThrow(() => assertEmbeddedPortraitMatches(denver, voteFor(proofs.denver2020, proofs.denver2020.portrait), proofs));
});

test('publication validator rejects misbound jurisdiction, voter, source and image', () => {
  const vote = voteFor(proofs.denver2020, proofs.denver2020.portrait);
  assert.throws(() => assertEmbeddedPortraitMatches({ jurisdiction_ref: 'us-co-place-0816495' }, vote, proofs), /identity/);
  for (const change of [
    (v) => { v.person_id += 1; },
    (v) => { v.name = 'Another person'; },
    (v) => { v.portrait.source_document_sha256 = '0'.repeat(64); },
    (v) => { v.portrait.source_url = 'https://example.com/'; },
    (v) => { v.portrait.source_page += 1; },
    (v) => { v.portrait.embedded_image_index += 1; },
    (v) => { v.portrait.sha256 = '0'.repeat(64); },
    (v) => { v.portrait.embedded_original_sha256 = '0'.repeat(64); },
  ]) {
    const changed = structuredClone(vote);
    change(changed);
    assert.throws(() => assertEmbeddedPortraitMatches(denver, changed, proofs), /identity/);
  }
});
