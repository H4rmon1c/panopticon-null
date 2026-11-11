import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, mkdtempSync, mkdirSync, writeFileSync, symlinkSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { buildCoverage, boundedJson, writeCoverage } from './build-coverage-api.mjs';

const snapshot = JSON.parse(readFileSync(new URL('../public/public-snapshot.json', import.meta.url)));
const geography = JSON.parse(readFileSync(new URL('../geography/colorado-jurisdictions.json', import.meta.url)));

test('real release counts decisions, not map entries or held cases', () => {
  const report = buildCoverage(snapshot, geography);
  assert.equal(report.coverage_state, 'partial');
  assert.equal(report.lookback_period, null);
  assert.equal(report.totals.geography_entries, 4111);
  assert.equal(report.totals.published_decisions, 5);
  assert.equal(report.totals.jurisdictions_with_published_records, 4);
  assert.equal(report.totals.jurisdictions_without_indexed_records, 4107);
  assert.equal(report.totals.direct_vote_rows, 47);
  assert.equal(report.totals.consent_motion_rows, 0);
  assert.equal(report.jurisdictions.find(row => row.id === 'us-co-place-0812415').coverage_state, 'published_records');
  assert.equal(report.jurisdictions.find(row => row.id === 'us-co-place-0870360').coverage_state, 'records_not_indexed');
  assert.deepEqual(buildCoverage(snapshot, geography), report);
  assert.equal(JSON.stringify(report).includes('reviewer'), false);
});

test('synthetic and preview snapshots never become coverage', () => {
  for (const key of ['demo', 'preview']) {
    const changed = structuredClone(snapshot);
    changed[key] = true;
    assert.throws(() => buildCoverage(changed, geography));
  }
});

test('ambiguous or missing geographic bindings fail instead of hiding published decisions', () => {
  const duplicated = structuredClone(geography);
  duplicated.jurisdictions.push(duplicated.jurisdictions[0]);
  assert.throws(() => buildCoverage(snapshot, duplicated), /duplicate geography/);
  const missing = structuredClone(geography);
  missing.jurisdictions = missing.jurisdictions.filter(row => row.id !== 'us-co-place-0816000');
  missing.counts.municipality--;
  assert.throws(() => buildCoverage(snapshot, missing), /missing a geography binding/);
  const wrong = structuredClone(geography);
  wrong.counts.county++;
  assert.throws(() => buildCoverage(snapshot, wrong), /counts differ/);
});

test('unsafe input files and API directory symlinks are rejected', () => {
  const dir = mkdtempSync(join(tmpdir(), 'pnull-coverage-'));
  try {
    writeFileSync(join(dir, 'large.json'), ' '.repeat(200));
    assert.throws(() => boundedJson(join(dir, 'large.json'), 20), /bounded regular file/);
    symlinkSync(join(dir, 'large.json'), join(dir, 'link.json'));
    assert.throws(() => boundedJson(join(dir, 'link.json'), 1000));
    mkdirSync(join(dir, 'public'));
    mkdirSync(join(dir, 'geography'));
    mkdirSync(join(dir, 'outside'));
    writeFileSync(join(dir, 'public/public-snapshot.json'), JSON.stringify(snapshot));
    writeFileSync(join(dir, 'geography/colorado-jurisdictions.json'), JSON.stringify(geography));
    symlinkSync(join(dir, 'outside'), join(dir, 'api'));
    assert.throws(() => writeCoverage(dir), /real directory/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('metadata cannot smuggle nested private fields or misleading text', () => {
  const changed = structuredClone(snapshot);
  changed.status.dataset_version = { private_note: 'not public' };
  assert.throws(() => buildCoverage(changed, geography));
  const bad = structuredClone(geography);
  bad.source_archived_on = { private_note: 'not public' };
  assert.throws(() => buildCoverage(snapshot, bad));
  const bidi = structuredClone(geography);
  bidi.jurisdictions[0].name = 'Colorado\u202e';
  assert.throws(() => buildCoverage(snapshot, bidi));
});
