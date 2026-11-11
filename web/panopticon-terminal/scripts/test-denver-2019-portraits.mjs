import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { validateResearch } from '../research-api.js';

const root = new URL('../../../', import.meta.url);
const proof = JSON.parse(await readFile(new URL('fixtures/atlas/portrait-sources/denver-2019-budget-proof.json', root), 'utf8'));
const report = JSON.parse(await readFile(new URL('../public/research-signals.json', import.meta.url), 'utf8'));
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const votes = (data) => data.records.filter((record) => record.jurisdiction_ref === 'us-co-place-0820000')
  .flatMap((record) => (record.actions ?? []).flatMap((motion) => motion.votes));

test('Denver budget portraits bind to exact portal voters and original image streams', async () => {
  assert.equal(proof.source_page, 6);
  assert.equal(proof.portraits.length, 10);
  assert.equal(hash(await readFile(new URL(proof.page_fixture, root))), proof.page_fixture_sha256);
  const changed = structuredClone(report);
  for (const item of proof.portraits) {
    const matches = votes(changed).filter((vote) => vote.person_id === item.person_id && vote.name === item.name);
    assert.ok(matches.length, `missing source vote for ${item.name}`);
    assert.equal(hash(await readFile(new URL(`../research-portraits/${item.sha256}.jpg`, import.meta.url))), item.sha256);
    for (const vote of matches) vote.portrait = {
      src: `./research-portraits/${item.sha256}.jpg`, sha256: item.sha256, source_kind: 'embedded_pdf',
      source_url: proof.source_url, source_document_sha256: proof.source_sha256, source_page: 6,
      embedded_image_index: item.image_index, caption: `2019 Denver budget; source labels this portrait ${item.source_label}.`,
      portrait_date: 'not_established',
    };
    if (item.person_id === 282) for (const vote of matches) vote.portrait.identity_source_url = proof.alias_source_url;
  }
  validateResearch(changed);
  const first = votes(changed).find((vote) => vote.person_id === 280);
  const second = votes(changed).find((vote) => vote.person_id === 251);
  first.portrait = second.portrait;
  assert.throws(() => validateResearch(changed), /portrait/i);
  const alteredAlias = structuredClone(report);
  const chris = proof.portraits.find((item) => item.person_id === 282);
  const chrisVote = votes(alteredAlias).find((vote) => vote.person_id === 282);
  chrisVote.portrait = { src: `./research-portraits/${chris.sha256}.jpg`, sha256: chris.sha256,
    source_kind: 'embedded_pdf', source_url: proof.source_url, source_document_sha256: proof.source_sha256,
    source_page: 6, embedded_image_index: chris.image_index,
    caption: `2019 Denver budget; source labels this portrait ${chris.source_label}.`,
    portrait_date: 'not_established', identity_source_url: 'https://example.com/wrong' };
  assert.throws(() => validateResearch(alteredAlias), /portrait/i);
});

test('Denver 2020 portrait binds a lossless display image to Candi CdeBaca', async () => {
  const source = JSON.parse(await readFile(new URL('fixtures/atlas/portrait-sources/denver-2020-budget-proof.json', root), 'utf8'));
  const item = source.portrait;
  assert.equal(hash(await readFile(new URL(source.page_fixture, root))), source.page_fixture_sha256);
  assert.equal(hash(await readFile(new URL(`../research-portraits/${item.sha256}.png`, import.meta.url))), item.sha256);
  const changed = structuredClone(report);
  const matches = votes(changed).filter((vote) => vote.person_id === item.person_id && vote.name === item.name);
  assert.ok(matches.length);
  for (const vote of matches) vote.portrait = {
    src: `./research-portraits/${item.sha256}.png`, sha256: item.sha256, source_kind: 'embedded_pdf',
    source_url: source.source_url, source_document_sha256: source.source_sha256, source_page: 6,
    embedded_image_index: item.image_index, embedded_original_sha256: item.original_sha256,
    image_transform: 'lossless_png_from_jpeg2000',
    caption: '2020 Denver budget; source labels this portrait Candi CdeBaca.', portrait_date: 'not_established',
  };
  validateResearch(changed);
  matches[0].portrait.embedded_original_sha256 = '0'.repeat(64);
  assert.throws(() => validateResearch(changed), /portrait/i);
});
