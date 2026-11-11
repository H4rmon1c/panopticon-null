import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { matchesResearchQuery, readBoundedResearchResponse, validateResearch, validResearchJurisdiction } from '../research-api.js';

const original = JSON.parse(await readFile(new URL('../public/research-signals.json', import.meta.url), 'utf8'));
const copy = () => structuredClone(original);

test('Colorado Springs original index, redirect receipt, and landing chain remain source scoped', () => {
  const report = copy();
  const record = report.records.find((r) => r.kind === 'meeting_document');
  assert.ok(record);
  record.jurisdiction_ref = 'us-co-place-0816000';
  record.source_url = 'https://coloradosprings.gov/system/files/parks_board_october_2022.draft_.pdf';
  record.document_format = 'pdf';
  delete record.capture_adjustment; delete record.indexed_source_url; delete record.source_index_url;
  record.springs_source_chain = {
    original_index_url: 'https://coloradosprings.gov/parksboard', original_index_sha256: 'a'.repeat(64),
    original_anchor_index: 3,
    original_document_url: 'https://coloradosprings.gov/sites/default/files/parks_board_october_2022.draft_.pdf',
    redirect_status: 301, redirect_captured_at: '2026-11-11T12:00:00Z', redirect_receipt_sha256: 'b'.repeat(64),
    landing_url: 'https://coloradosprings.gov/document/parksboardoctober2022.draft.pdf', landing_sha256: 'c'.repeat(64),
    landing_anchor_index: 1,
  };
  report.coverage.jurisdictions_with_mentions = new Set(report.records.map((item) => item.jurisdiction_ref)).size;
  assert.doesNotThrow(() => validateResearch(report));
  for (const change of [
    (r) => { r.jurisdiction_ref = 'us-co-place-0816495'; },
    (r) => { r.springs_source_chain.original_index_url = 'https://other.example/parks'; },
    (r) => { r.springs_source_chain.original_index_url += '?token=secret'; },
    (r) => { r.springs_source_chain.original_document_url = 'https://coloradosprings.gov/sites/default/files/other.pdf'; },
    (r) => { r.springs_source_chain.landing_url = 'https://coloradosprings.gov/document/other.pdf'; },
    (r) => { r.springs_source_chain.redirect_receipt_sha256 = 'no'; },
    (r) => { r.springs_source_chain.redirect_status = 200; },
    (r) => { r.springs_source_chain.original_anchor_index = -1; },
    (r) => { r.source_url = 'https://coloradosprings.gov/system/files/other.pdf'; },
  ]) {
    const changed = structuredClone(report);
    const row = changed.records.find((r) => r.id === record.id);
    change(row);
    assert.throws(() => validateResearch(changed));
  }
});

test('cross-agency portrait requires the exact authority, person and pinned source chain', () => {
  const report = copy();
  const record = report.records.find((r) => r.actions?.some((a) => a.votes.some((v) => v.person_id === 637 && v.name === 'Craig Machuga')));
  assert.ok(record);
  const motion = record.actions.find((a) => a.votes.some((v) => v.person_id === 637));
  const vote = motion.votes.find((v) => v.person_id === 637);
  vote.portrait = { src: './research-portraits/03afcbdf3dcadad64a2817377403fc0e49cd55a9b6ec6855b4e78f33524e5a3f.jpg', sha256: '03afcbdf3dcadad64a2817377403fc0e49cd55a9b6ec6855b4e78f33524e5a3f',
    source_url: 'https://www.sacfd.org/meet-our-district-board-of-directors',
    image_url: 'https://streamline.imgix.net/0ec9c10d-3273-4b87-a0de-62fc6b0603cd/9a0b203c-7e23-4e21-8628-e6390fbdd0c9/C.%20Machuga.JPG?ixlib=rb-1.1.0&or=0&w=720&h=720&fit=fill&fill=blur&auto=format%2Ccompress&s=066395a71136c9612a133dc915584835',
    identity_source_url: 'https://www.c3gov.com/Government/Boards-Commissions/Urban-Renewal-Authority', identity_source_sha256: 'b7f94b19ff9a65699174df106a141fb10d3a2328b4dc3e6cd64b640be6423403',
    identity_agenda_url: 'https://ompnetwork.s3-us-west-2.amazonaws.com/sites/1717/documents/agenda_22_1.pdf', identity_agenda_sha256: '2936f9b6bee4c0a1b1556579dd8fb807fa0bfc5a7ec357e1fb22e352ec39a67a',
    identity_matter_url: 'https://webapi.legistar.com/v1/commerce/Matters/9151', identity_matter_sha256: 'a73a53441b86e65b3a81080f34dfe76563c203a9530d65a2a0ea7ff87741358f',
    portrait_source_sha256: 'c62be165e986063ecd8b955dd3ee730516f92268d823060cc1d40d2abd79609b',
    caption: 'Fire board name-labeled card shows Craig Machuga; page 1 of the 2022 appointment agenda lists a South Adams Fire Department recommendation. Identity connection is preliminary.', portrait_date: 'not_established' };
  validateResearch(report);
  for (const change of [
    (v, a) => { a.body = 'City Council'; },
    (v) => { v.person_id = 638; },
    (v) => { v.portrait.identity_agenda_sha256 = '0'.repeat(64); },
    (v) => { v.portrait.portrait_source_sha256 = '0'.repeat(64); },
    (v) => { v.portrait.image_url = 'https://example.com/wrong.jpg'; },
  ]) {
    const changed = structuredClone(report);
    const action = changed.records.find((r) => r.id === record.id).actions.find((a) => a.votes.some((v) => v.person_id === 637));
    change(action.votes.find((v) => v.person_id === 637), action);
    assert.throws(() => validateResearch(changed));
  }
});

test('real research output remains explicitly preliminary and counts source references', () => {
  const report = validateResearch(copy());
  assert.equal(report.state, 'preliminary');
  assert.equal(report.coverage.lookback_period, null);
  assert.ok(report.records.length > 0);
  assert.ok(report.records.every((record) => record.confidence === 'unconfirmed_source_reference'));
  assert.equal(new Set(report.records.map((record) => record.id)).size, report.records.length);
});

test('special-district references and jurisdiction filters use exact five-digit IDs', () => {
  const report = copy();
  const record = report.records.find((item) => item.kind === 'meeting_document');
  assert.ok(record);
  record.jurisdiction_ref = 'dola-district-18053';
  report.coverage.jurisdictions_with_mentions = new Set(report.records.map((item) => item.jurisdiction_ref)).size;
  report.coverage.search_attempts ??= [];
  report.coverage.search_attempts = report.coverage.search_attempts.filter((attempt) => attempt.jurisdiction_ref !== record.jurisdiction_ref);
  report.coverage.search_attempts.push({ jurisdiction_ref: record.jurisdiction_ref, checked_at: '2026-11-11T00:00:00Z',
    website_check: 'site_checked', documents_screened: 1, documents_failed: 0, documents_queued: 0,
    portal_pages_checked: 0, agenda_meeting_pages_screened: 0, lookback_period: null });
  assert.equal(validResearchJurisdiction(record.jurisdiction_ref), true);
  validateResearch(report);
  for (const bad of ['dola-district-1805', 'dola-district-180530', 'dola-district-18x53', 'dola-district-18053/other']) {
    assert.equal(validResearchJurisdiction(bad), false);
    const changed = structuredClone(report);
    changed.coverage.search_attempts.at(-1).jurisdiction_ref = bad;
    assert.throws(() => validateResearch(changed));
  }
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
    (r) => { r.coverage.ocr_revalidation_pending = -1; },
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

test('redirected documents expose the exact source link and timestamp uncertainty', () => {
  const base = copy();
  const record = base.records.find((item) => item.kind === 'meeting_document');
  record.document_format = 'pdf';
  record.source_url = 'https://cms3.revize.com/revize/town/Documents/Meeting%20Minutes.pdf';
  record.indexed_source_url = 'https://example.org/Documents/Meeting%20Minutes.pdf?t=123';
  record.source_index_url = 'https://example.org/agendas';
  record.capture_adjustment = 'timestamp_omitted';
  validateResearch(base);
  for (const change of [
    (item) => { item.source_url = item.source_url.replace('cms3.revize.com', 'attacker.example'); },
    (item) => { item.source_url = item.source_url.replace('Meeting%20Minutes', 'Other'); },
    (item) => { item.indexed_source_url = item.indexed_source_url.replace('example.org', 'attacker.example'); },
    (item) => { item.indexed_source_url += '&another=1'; },
    (item) => { item.capture_adjustment = 'direct_redirect'; },
    (item) => { delete item.source_index_url; },
  ]) {
    const changed = structuredClone(base);
    change(changed.records.find((item) => item.id === record.id));
    assert.throws(() => validateResearch(changed));
  }
  record.source_url = 'https://example.org/Documents/Meeting%20Minutes.pdf?t=123';
  delete record.indexed_source_url;
  delete record.source_index_url;
  delete record.capture_adjustment;
  validateResearch(base);
  record.source_url += '&redirect=x';
  assert.throws(() => validateResearch(base));
});

test('TownCloud redirects stay bound to the government link and exact tenant', () => {
  const base = copy();
  const record = base.records.find((item) => item.kind === 'meeting_document');
  record.document_format = 'pdf';
  record.source_url = 'https://towncloud-core-prod.s3.us-west-2.amazonaws.com/uploads/walden-co/meetings/agenda_packet/pdf/289/meeting.pdf';
  record.indexed_source_url = 'https://towncloud.io/go/walden-co/agendas/293/PDF/get_agenda_document_link';
  record.source_index_url = 'https://town.example.org/agendas';
  record.capture_adjustment = 'direct_redirect';
  validateResearch(base);
  for (const change of [
    (item) => { item.source_url = item.source_url.replace('/walden-co/', '/other/'); },
    (item) => { item.source_url = item.source_url.replace('towncloud-core-prod.s3.us-west-2.amazonaws.com', 'attacker.example'); },
    (item) => { item.indexed_source_url = item.indexed_source_url.replace('towncloud.io', 'attacker.example'); },
    (item) => { item.indexed_source_url += '?other=1'; },
    (item) => { item.capture_adjustment = 'timestamp_omitted'; },
    (item) => { item.source_url += '?secret=1'; },
  ]) {
    const changed = structuredClone(base);
    change(changed.records.find((item) => item.id === record.id));
    assert.throws(() => validateResearch(changed));
  }
});

test('additional document passages remain bounded, distinct and source-located', () => {
  const base = copy();
  const record = base.records.find((item) => item.kind === 'meeting_document' && !item.ocr);
  record.document_highlights = [{ locator: 'extracted line 999', title: 'Resolution about Flock cameras.', signals: ['Flock Safety'] }];
  validateResearch(base);
  for (const change of [
    (item) => { item.document_highlights[0].locator = item.locator; },
    (item) => { item.document_highlights[0].title = 'bad\u202Etext'; },
    (item) => { item.document_highlights[0].signals = []; },
    (item) => { item.document_highlights[0].private_note = 'not for publication'; },
    (item) => { item.ocr = true; },
    (item) => { item.document_highlights = Array(6).fill(item.document_highlights[0]); },
  ]) {
    const changed = structuredClone(base);
    change(changed.records.find((item) => item.id === record.id));
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
    (report) => { report.coverage.search_attempts[0].documents_redirected = -1; },
    (report) => { report.coverage.search_attempts[0].historical_index_pages_checked = -1; },
    (report) => { report.coverage.search_attempts[0].historical_index_errors = -1; },
    (report) => { report.coverage.search_attempts[0].ocr_revalidation_pending = 3; },
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
