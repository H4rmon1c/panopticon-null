import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { votePortraits } from '../portraits/portraits.js';
import { matchesDenverGallery, lookupPortrait, lookupResearchPortrait } from '../portraits/lookup.js';

const base = new URL('../../../', import.meta.url);
const registry = JSON.parse(await readFile(new URL('fixtures/atlas/portrait-sources/denver-2023-portrait-registry.json', base), 'utf8'));
const html = await readFile(new URL('fixtures/atlas/portrait-sources/denver-2023-council-gallery.html', base), 'utf8');
const snapshot = JSON.parse(await readFile(new URL('../public/public-snapshot.json', import.meta.url), 'utf8'));
const decision = snapshot.entities.find((item) => item.slug?.startsWith('co-denver-25-0413-'));
const entities = new Map(snapshot.entities.map((item) => [item.id, item]));

test('all named Denver roll-call voters bind to their exact official gallery portrait', () => {
  assert.equal(decision.votes.length, 13);
  for (const vote of decision.votes) {
    const portrait = votePortraits[vote.voter_entity_id];
    assert.deepEqual(portrait, registry[vote.voter_entity_id]);
    const voter = entities.get(vote.voter_entity_id);
    assert.equal(lookupPortrait(votePortraits, vote, voter), portrait);
    assert.equal(matchesDenverGallery(portrait, voter, html), true);
  }
});

test('changed name, image, jurisdiction, caption or page does not create a portrait match', () => {
  const vote = decision.votes[0];
  const portrait = votePortraits[vote.voter_entity_id];
  const voter = entities.get(vote.voter_entity_id);
  for (const change of [
    { name: 'Another Person' },
    { image_url: votePortraits[decision.votes[1].voter_entity_id].image_url },
    { source_image_alt: 'Another Person, District 7' },
    { source_page_locator: 'unrelated element' },
    { source_url: 'https://elsewhere.example/gallery' },
  ]) assert.equal(matchesDenverGallery({ ...portrait, ...change }, voter, html), false);
  assert.equal(matchesDenverGallery(portrait, { ...voter, identity_scope: 'legistar:commerce' }, html), false);
  assert.equal(matchesDenverGallery(portrait, voter, html.replace(portrait.source_image_alt, 'Someone Else, District 7')), false);
  assert.equal(lookupPortrait(votePortraits, vote, { ...voter, official_person_id: '999' }), null);
});

test('research votes reuse sourced portraits only for the exact portal person and name', () => {
  const vote = decision.votes[0];
  const portrait = votePortraits[vote.voter_entity_id];
  const researchVote = { name: vote.name, person_id: Number(portrait.official_person_id) };
  assert.equal(lookupResearchPortrait(votePortraits, 'denver', portrait.jurisdiction_ref, researchVote), portrait);
  assert.equal(lookupResearchPortrait(votePortraits, 'commerce', portrait.jurisdiction_ref, researchVote), null);
  assert.equal(lookupResearchPortrait(votePortraits, 'denver', 'us-co-other', researchVote), null);
  assert.equal(lookupResearchPortrait(votePortraits, 'denver', portrait.jurisdiction_ref, { ...researchVote, name: 'Another Person' }), null);
  assert.equal(lookupResearchPortrait(votePortraits, 'denver', portrait.jurisdiction_ref, { ...researchVote, person_id: 999999 }), null);
});
