import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { validateResearch } from '../research-api.js';

const base = JSON.parse(await readFile(new URL('../public/research-signals.json', import.meta.url), 'utf8'));
const seeds = [
  ['sterling-bwc', 'us-co-place-0873935', 'Sterling', 'https://www.sterlingcolo.com/departments/police_department/officer_body-worn_cameras.php', 'Sterling police describe their body-worn camera program.', ['Body-worn cameras']],
  ['greeley-flock', 'us-co-place-0832155', 'Greeley', 'https://greeleyco.gov/police/technology/automated-license-plate-reader-program/', 'Greeley police describe their Flock Safety camera program.', ['Flock Safety']],
  ['boulder-flock', 'us-co-place-0807850', 'Boulder', 'https://bouldercolorado.gov/services/flock-safety-cameras-and-boulder-police-department', 'Boulder police describe their selection of Flock Safety.', ['Flock Safety']],
  ['broomfield-flock', 'us-co-place-0809280', 'Broomfield', 'https://www.broomfield.org/4545/Flock-Safety-License-Plate-Readers', 'Broomfield police describe their Flock Safety license-plate-reader program.', ['Flock Safety', 'Automated License Plate Readers']],
  ['boulder-uas', 'us-co-place-0807850', 'Boulder', 'https://bouldercolorado.gov/services/unmanned-aircraft-system', 'Boulder police describe their unmanned-aircraft program.', ['Police drone']],
  ['westminster-axon', 'us-co-place-0883835', 'Westminster', 'https://www.westminsterco.gov/180/Engagement-Transparency', 'Westminster police describe their Axon body-worn-camera program.', ['Axon', 'Body-worn cameras']],
];
function report() {
  const data = structuredClone(base);
  data.records = data.records.filter((item) => item.kind !== 'official_page_context');
  data.records.push(...seeds.map(([id, ref, place, url, title, signals]) => ({
    id: `co-official-context-${id}`, kind: 'official_page_context', confidence: 'unconfirmed_source_reference',
    context_state: 'active',
    jurisdiction_ref: ref, jurisdiction: place, file: 'Official police technology page', title, signals,
    source_status: '', source_url: url, source_sha256: 'a'.repeat(64), locator: 'HTML text node 13',
    captured_at: '2026-11-11T12:00:00Z', last_screened_at: '2026-11-11T12:00:00Z',
  })));
  data.coverage.official_page_references = seeds.length;
  data.coverage.jurisdictions_with_mentions = new Set(data.records.map((item) => item.jurisdiction_ref)).size;
  return data;
}
test('six official technology pages validate only as context', () => {
  assert.equal(validateResearch(report()).coverage.official_page_references, 6);
});
test('research page loads matching versioned modules', async () => {
  const html = await readFile(new URL('../research.html', import.meta.url), 'utf8');
  const script = await readFile(new URL('../research.js', import.meta.url), 'utf8');
  assert.match(html, /research\.js\?v=gunnison-motion-1/);
  assert.match(script, /research-api\.js\?v=garden-motion-1/);
});
test('Sterling context cannot be swapped or given an invented approval', () => {
  for (const change of [
    (r) => { r.jurisdiction_ref = 'us-co-place-0807850'; },
    (r) => { r.source_url = 'https://www.sterlingcolo.com.evil.example/'; },
    (r) => { r.title = 'Sterling approved a camera contract.'; },
    (r) => { r.actions = []; },
    (r) => { r.signals = ['Automated License Plate Readers']; },
  ]) {
    const data = report(); change(data.records.find((r) => r.id === 'co-official-context-sterling-bwc'));
    assert.throws(() => validateResearch(data));
  }
});
test('official context rejects swapped agency, destination, claims, markup and invented votes', () => {
  for (const change of [
    (r) => { r.jurisdiction_ref = 'us-co-place-0807850'; },
    (r) => { r.source_url = 'https://www.broomfield.org.evil.example/path'; },
    (r) => { r.title = '<img src=x onerror=alert(1)>'; },
    (r) => { r.signals = ['approved']; },
    (r) => { r.file = 'Contract approval'; },
    (r) => { r.actions = []; },
    (r) => { r.people_context = []; },
    (r) => { r.locator = 'HTML text node 0'; },
  ]) {
    const data = report(); change(data.records.at(-3));
    assert.throws(() => validateResearch(data));
  }
});
test('new agency contexts cannot be swapped or given invented actions', () => {
  for (const index of [-5, -4]) {
    for (const change of [
      (r) => { r.jurisdiction = 'Other city'; },
      (r) => { r.source_url = 'https://bouldercolorado.gov/another-page'; },
      (r) => { r.title = 'The city approved a contract.'; },
      (r) => { r.signals = ['Flock Safety', 'Automated License Plate Readers']; },
      (r) => { r.actions = []; },
    ]) {
      const data = report(); change(data.records.at(index));
      assert.throws(() => validateResearch(data));
    }
  }
});
test('withdrawal keeps stable record without asserting the old technology claim', () => {
  const data = report(); const record = data.records.at(-3); const id = record.id;
  record.context_state = 'withdrawn';
  record.title = 'Official agency page changed; prior technology context withdrawn.';
  record.signals = ['Source changed']; record.locator = 'Source response (withdrawal)';
  data.coverage.jurisdictions_with_mentions = new Set(data.records.filter((item) => item.context_state !== 'withdrawn').map((item) => item.jurisdiction_ref)).size;
  assert.equal(validateResearch(data).records.at(-3).id, id);
  record.title = 'Broomfield police describe their Flock Safety license-plate-reader program.';
  assert.throws(() => validateResearch(data));
});
test('withdrawal marker cannot be attached to another kind of record', () => {
  const data = structuredClone(base);
  data.records[0].context_state = 'withdrawn';
  assert.throws(() => validateResearch(data));
});
