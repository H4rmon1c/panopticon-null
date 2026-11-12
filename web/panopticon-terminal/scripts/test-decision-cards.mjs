import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { decisionCardModels, decisionHref, displayVote, linkedDecisions, matchesDecisionCard, outcomeLabel, previewPeople, voteSummary } from './25-decision-cards.mjs';

const snapshot = JSON.parse(readFileSync(new URL('../public/public-snapshot.json', import.meta.url), 'utf8'));
const cards = decisionCardModels(snapshot);

test('real released records show their correct outcomes and scoped votes', () => {
  assert.equal(cards.length, 5);
  const denver = cards.find((c) => c.record.slug === 'co-denver-25-0413-2025-05-05');
  assert.equal(denver.outcome, 'Failed');
  assert.match(denver.tally, /12 No/);
  assert.match(denver.tally, /1 Absent/);
  const springs = cards.find((c) => c.record.slug === 'co-colorado-springs-25-581-2025-11-25');
  assert.equal(springs.outcome, 'Passed');
  assert.match(springs.tally, /6 Yes/);
  assert.match(springs.tally, /3 No/);
  assert.match(springs.title, /surcharge/i);
  assert.ok(springs.people.some((p) => p.portrait && p.vote.name === 'Tom Bailey'));
  assert.ok(springs.href.endsWith('/co-colorado-springs-25-581-2025-11-25.html'));
});

test('search matches places and voter names without treating a missing result as absence', () => {
  assert.ok(cards.some((c) => matchesDecisionCard(c, 'Castle Rock, CO')));
  assert.ok(cards.some((c) => matchesDecisionCard(c, 'Castle Rock CO')));
  assert.ok(cards.some((c) => matchesDecisionCard(c, 'Tom Bailey')));
  assert.ok(!cards.some((c) => matchesDecisionCard(c, 'Somewhere nowhere')));
});

test('consent and missing data never invent an individual vote or outcome', () => {
  assert.match(voteSummary({consent_block: {roll_call:[{vote:'Aye'}]}}), /Consent agenda/);
  assert.equal(outcomeLabel({consent_block: {}, attributes:[{label:'Outcome',value:'Pass'}]}), 'Consent agenda');
  assert.equal(voteSummary({}), 'Individual votes not established');
  assert.equal(outcomeLabel({}), 'Outcome not established');
  assert.equal(displayVote('Aye'), 'Yes');
  assert.equal(displayVote('Nay'), 'No');
  assert.equal(displayVote('Absent'), 'Absent');
});

test('older voter permalinks lead to every matching, scoped vote record', () => {
  const matches = linkedDecisions(snapshot, 'public', 'recorded_voter_co_611');
  assert.equal(matches.length, 1);
  assert.match(matches[0].href, /colorado-springs.*#evidence-evidence_vote_84638$/);
  const douglas = linkedDecisions(snapshot, 'public', 'recorded_voter_co_commerce_city_commerce_210');
  assert.equal(douglas.length, 2);
  assert.ok(douglas.every((m) => /#evidence-/.test(m.href)));
  assert.deepEqual(linkedDecisions(snapshot, 'public', 'nonexistent'), []);
});

test('a searched voter appears in the photo preview even beyond the first four', () => {
  const springs = cards.find((card) => card.record.slug === 'co-colorado-springs-25-581-2025-11-25');
  assert.equal(previewPeople(springs, 'Leinweber')[0].vote.name, 'David Leinweber');
  assert.equal(previewPeople(springs, 'Colorado Springs')[0].vote.name, 'Tom Bailey');
});

test('a corrected decision takes precedence over the editorial summary', () => {
  const revised = structuredClone(snapshot);
  const springs = revised.entities.find((e) => e.slug === 'co-colorado-springs-25-581-2025-11-25');
  springs.description = 'A corrected account.';
  const card = decisionCardModels(revised).find((c) => c.record.id === springs.id);
  assert.equal(card.title, springs.name);
  assert.equal(card.summary, 'A corrected account.');
  const revisedOutcome = structuredClone(snapshot);
  const revisedSprings = revisedOutcome.entities.find((e) => e.slug === 'co-colorado-springs-25-581-2025-11-25');
  revisedSprings.attributes.find((a) => a.label === 'Outcome').value = 'Fail';
  const changed = decisionCardModels(revisedOutcome).find((c) => c.record.id === revisedSprings.id);
  assert.equal(changed.title, revisedSprings.name);
  assert.equal(changed.outcome, 'Failed');
});

test('unsafe slugs cannot become links and demo records remain in demo mode', () => {
  assert.equal(decisionHref({slug:'../bad'}), null);
  assert.equal(decisionHref({slug:'sample'}, 'demo'), './decision.html?decision=sample&demo=1');
  assert.ok(decisionCardModels(snapshot, 'demo').every((c) => c.people.every((p) => !p.portrait)));
});
