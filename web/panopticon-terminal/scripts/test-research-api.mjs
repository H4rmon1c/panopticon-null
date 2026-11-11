import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { matchesResearchQuery, readBoundedResearchResponse, validateResearch } from '../research-api.js';

const original = JSON.parse(await readFile(new URL('../public/research-signals.json', import.meta.url), 'utf8'));
const copy = () => structuredClone(original);

test('real research output remains explicitly preliminary and counts source references', () => {
  const report = validateResearch(copy());
  assert.equal(report.state, 'preliminary');
  assert.equal(report.coverage.lookback_period, null);
  assert.ok(report.records.length > 0);
  assert.ok(report.records.every((record) => record.confidence === 'unconfirmed_source_reference'));
  assert.equal(new Set(report.records.map((record) => record.id)).size, report.records.length);
});

test('research search includes people named in recorded votes', () => {
  const item = original.records.find((record) => record.actions?.some((action) => action.votes.length));
  const name = item.actions.find((action) => action.votes.length).votes[0].name;
  assert.equal(matchesResearchQuery(item, name.toUpperCase()), true);
  assert.equal(matchesResearchQuery(item, 'unrelated-nonexistent-person-name'), false);
});

test('invalid confidence, source URL, identifiers, counts and text are rejected', () => {
  const changes = [
    (r) => { r.records[0].confidence = 'verified'; },
    (r) => { r.records[0].source_url = 'http://127.0.0.1/admin'; },
    (r) => { r.records[0].source_url = 'https://172.16.0.1/secret.pdf'; },
    (r) => { r.records[0].matter_url = 'https://webapi.legistar.com/v1/other/Matters/42'; },
    (r) => { r.records[0].source_url = 'https://u:p@webapi.legistar.com/v1/x'; },
    (r) => { r.records[0].source_sha256 = 'not-a-digest'; },
    (r) => { r.records[0].jurisdiction = 'Colorado\u202eSpoof'; },
    (r) => { r.records[0].locator = '$[0].MatterVotes'; },
    (r) => { r.records.push(r.records[0]); },
    (r) => { r.coverage.matter_references++; },
    (r) => { r.coverage.lookback_period = 'all time'; },
    (r) => { r.private_note = 'not for publication'; },
    (r) => { r.records[0].private_note = 'not for publication'; },
    (r) => { r.coverage.document_screening.internal_error = 2; },
    (r) => { r.coverage.non_surveillance_mentions_withheld = -1; },
  ];
  for (const change of changes) { const report = copy(); change(report); assert.throws(() => validateResearch(report)); }
});

test('missing or dishonest content-length cannot bypass bounded response reading', async () => {
  const json = JSON.stringify(original);
  const response = new Response(json);
  assert.equal((await readBoundedResearchResponse(response, 8 * 1024 * 1024)).records.length, original.records.length);
  const stream = new ReadableStream({ start(controller) { controller.enqueue(new Uint8Array(40)); controller.close(); } });
  await assert.rejects(readBoundedResearchResponse(new Response(stream), 20), /exceeds/);
  await assert.rejects(readBoundedResearchResponse(new Response('{}', { headers: { 'content-length': '900' } }), 20), /exceeds/);
});

test('recorded motions bind the same matter and distinguish consent votes', () => {
  const base = copy();
  for (const item of base.records) delete item.actions;
  const record = base.records.find((item) => item.kind === 'matter_reference');
  const [client, matter] = new URL(record.matter_url).pathname.match(/\/v1\/([^/]+)\/Matters\/(\d+)/).slice(1);
  const prefix = `https://webapi.legistar.com/v1/${client}`;
  record.actions = [{ date: '2026-01-15', body: 'Council', action: 'approved', outcome: 'Pass',
    vote_scope: 'consent_block', history_url: `${prefix}/Matters/${matter}/Histories`, history_sha256: 'a'.repeat(64),
    history_locator: '$[0]', item_url: `${prefix}/Events/11/EventItems/22`, item_sha256: 'b'.repeat(64),
    votes_url: `${prefix}/EventItems/22/Votes`, votes_sha256: 'c'.repeat(64), captured_at: '2026-11-11T00:00:00Z',
    block_url: `${prefix}/Events/11/EventItems/44`, block_sha256: 'd'.repeat(64),
    votes: [{ name: 'Alex Example', value: 'Aye', locator: '$[0]', person_id: 9, vote_id: 8 }] }];
  base.coverage.recorded_actions = 1;
  base.coverage.recorded_vote_entries = 1;
  validateResearch(base);
  const changes = [
    (motion) => { motion.history_url = `${prefix}/Matters/999999/Histories`; },
    (motion) => { motion.votes_url = `${prefix}/EventItems/999/Votes`; },
    (motion) => { motion.block_url = `${prefix}/Events/999/EventItems/44`; },
    (motion) => { motion.vote_scope = 'not_established'; },
    (motion) => { motion.votes[0].private_note = 'not for publication'; },
  ];
  for (const change of changes) {
    const changed = structuredClone(base);
    change(changed.records.find((item) => item.actions).actions[0]);
    assert.throws(() => validateResearch(changed));
  }
});

test('county-linked agenda references remain scoped to their exact meeting and digest', () => {
  const base = copy();
  const sha = 'e'.repeat(64);
  base.records.push({ id: `co-agendasuite-elpaso-123-${sha.slice(0, 12)}`, kind: 'agenda_portal_reference',
    confidence: 'unconfirmed_source_reference', jurisdiction_ref: 'us-co-county-08041', jurisdiction: 'El Paso County portal',
    file: 'Meeting 123', title: 'Possible Flock reference in a public meeting page.', signals: ['Flock'], source_status: '',
    source_url: 'https://www.agendasuite.org/iip/elpaso/meeting/details/123', source_sha256: sha,
    locator: 'HTML text node 1', captured_at: '2026-11-11T00:00:00Z', last_screened_at: '2026-11-11T00:00:00Z', historical: false });
  base.coverage.agenda_portal_references = (base.coverage.agenda_portal_references ?? 0) + 1;
  base.coverage.jurisdictions_with_mentions = new Set(base.records.map((item) => item.jurisdiction_ref)).size;
  validateResearch(base);
  for (const change of [
    (item) => { item.source_url = 'https://evil.example/iip/elpaso/meeting/details/123'; },
    (item) => { item.source_url = 'https://www.agendasuite.org/iip/elpaso/meeting/details/999'; },
    (item) => { item.source_sha256 = 'f'.repeat(64); },
    (item) => { item.jurisdiction_ref = 'us-co-county-08001'; },
  ]) {
    const changed = structuredClone(base);
    change(changed.records.at(-1));
    assert.throws(() => validateResearch(changed));
  }
});

test('partial jurisdiction attempts cannot claim a complete time window', () => {
  const base = copy();
  base.coverage.search_attempts = [{ jurisdiction_ref: 'us-co-county-08041', checked_at: '2026-11-11T00:00:00Z',
    website_check: 'site_checked', documents_screened: 12, documents_failed: 2, documents_queued: 5, portal_pages_checked: 0,
    agenda_meeting_pages_screened: 104, lookback_period: null }];
  validateResearch(base);
  for (const change of [
    (report) => { report.coverage.search_attempts[0].lookback_period = 'all time'; },
    (report) => { report.coverage.search_attempts[0].documents_screened = -1; },
    (report) => { report.coverage.search_attempts[0].private_note = 'not for publication'; },
    (report) => { report.coverage.search_attempts.push(report.coverage.search_attempts[0]); },
  ]) {
    const changed = structuredClone(base);
    change(changed);
    assert.throws(() => validateResearch(changed));
  }
});

test('named context never creates a recorded vote and portrait paths stay bound to digests', () => {
  const base = copy();
  const item = base.records.find((record) => record.people_context?.some((person) => person.portrait));
  assert.ok(item, 'captured official portrait context exists');
  const person = item.people_context.find((value) => value.portrait);
  assert.equal(person.individual_vote, 'not_established');
  validateResearch(base);
  for (const change of [
    (p) => { p.individual_vote = 'Aye'; },
    (p) => { p.portrait.src = './research-portraits/wrong.jpg'; },
    (p) => { p.portrait.image_url = 'https://127.0.0.1/image.jpg'; },
    (p) => { p.private_note = 'not for publication'; },
    (p) => { p.portrait.private_note = 'not for publication'; },
  ]) {
    const report = copy();
    const target = report.records.find((record) => record.people_context?.some((value) => value.portrait));
    change(target.people_context.find((value) => value.portrait));
    assert.throws(() => validateResearch(report));
  }
});
