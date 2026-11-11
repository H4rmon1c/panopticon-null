import { constants, closeSync, fstatSync, lstatSync, mkdirSync, openSync, readSync, renameSync, unlinkSync, writeFileSync } from 'node:fs';
import { createHash, randomUUID } from 'node:crypto';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { validateSnapshot } from '../api.js';
import { coverageDecisionIndex } from './24-colorado-coverage.mjs';

const KINDS = ['state', 'county', 'municipality', 'school_district', 'district_provisional'];
const digest = value => `sha256:${createHash('sha256').update(JSON.stringify(value)).digest('hex')}`;

export function buildCoverage(snapshot, geography) {
  validateSnapshot(snapshot);
  if (snapshot.demo !== false || snapshot.preview !== false || snapshot.status.state !== 'released') {
    throw new Error('Coverage requires a released public snapshot');
  }
  if (typeof snapshot.status.dataset_version !== 'string' || !/^sha256:[0-9a-f]{64}$/.test(snapshot.status.dataset_version) ||
      typeof geography?.source_archived_on !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(geography.source_archived_on) ||
      new Date(`${geography.source_archived_on}T00:00:00Z`).toISOString().slice(0, 10) !== geography.source_archived_on) {
    throw new Error('Coverage metadata must contain a digest and archive date');
  }
  const records = geography?.jurisdictions;
  if (geography?.schema_version !== 1 || !Array.isArray(records) || !records.length || records.length > 10000) {
    throw new Error('Invalid bounded geography roster');
  }
  const ids = new Set();
  const publishedIds = new Set();
  const counts = Object.fromEntries(KINDS.map(kind => [kind, 0]));
  for (const record of records) {
    if (!/^[a-z0-9-]{1,100}$/.test(record.id) || ids.has(record.id) || !KINDS.includes(record.kind) ||
        typeof record.name !== 'string' || !record.name.trim() || record.name.length > 300 || /[<>\x00-\x1f\x7f-\x9f\u202a-\u202e\u2066-\u2069]/u.test(record.name)) {
      throw new Error('Invalid or duplicate geography record');
    }
    if (record.published_entity_id && publishedIds.has(record.published_entity_id)) throw new Error('Ambiguous published geography identity');
    if (record.published_entity_id) publishedIds.add(record.published_entity_id);
    ids.add(record.id);
    counts[record.kind]++;
  }
  for (const kind of KINDS) {
    if (counts[kind] !== geography.counts?.[kind]) throw new Error('Geography counts differ from records');
  }
  const index = coverageDecisionIndex(records, { ...snapshot, mode: 'public' });
  const mapped = new Map();
  for (const [id, decisions] of index) {
    for (const decision of decisions) {
      if (mapped.has(decision.id) && mapped.get(decision.id) !== id) throw new Error('Decision maps to multiple geography records');
      mapped.set(decision.id, id);
    }
  }
  const decisions = snapshot.entities.filter(entity => entity.type === 'DECISION');
  if (decisions.some(decision => !mapped.has(decision.id))) throw new Error('Published decision is missing a geography binding');
  const jurisdictions = [...records].sort((a, b) => a.id.localeCompare(b.id, 'en')).map(record => {
    const local = index.get(record.id) ?? [];
    return {
      id: record.id, name: record.name, kind: record.kind,
      identity_state: record.kind === 'district_provisional' ? 'provisional' : 'geography_indexed',
      coverage_state: local.length ? 'published_records' : 'records_not_indexed',
      decision_ids: local.map(decision => decision.slug).sort(),
      direct_vote_rows: local.reduce((sum, decision) => sum + (decision.votes?.length ?? 0), 0),
      consent_motion_rows: local.reduce((sum, decision) => sum + (decision.consent_block?.roll_call?.length ?? 0), 0),
    };
  });
  const report = {
    schema_version: 'pnull-coverage-v1',
    snapshot_version: snapshot.status.dataset_version,
    coverage_state: 'partial',
    lookback_period: null,
    geography_archived_on: geography.source_archived_on,
    caveats: [
      'A geography entry is not an investigation. Records not indexed does not mean no surveillance technology exists.',
      'This roster is not a complete inventory of Colorado public bodies; provisional districts require identity verification.',
      'Source reach, current source-policy review, and live collection health are not established by this report.',
      'Consent-motion rows describe their block motion and are never attributed as item-specific votes.',
    ],
    totals: {
      geography_entries: records.length, by_kind: counts,
      published_decisions: decisions.length,
      jurisdictions_with_published_records: jurisdictions.filter(row => row.decision_ids.length).length,
      jurisdictions_without_indexed_records: jurisdictions.filter(row => !row.decision_ids.length).length,
      direct_vote_rows: jurisdictions.reduce((sum, row) => sum + row.direct_vote_rows, 0),
      consent_motion_rows: jurisdictions.reduce((sum, row) => sum + row.consent_motion_rows, 0),
    },
    jurisdictions,
  };
  return { ...report, version: digest(report) };
}

export function boundedJson(path, maximumBytes) {
  const fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    const stat = fstatSync(fd);
    if (!stat.isFile() || stat.size > maximumBytes) throw new Error('Input is not a bounded regular file');
    const bytes = Buffer.alloc(stat.size + 1);
    let offset = 0;
    while (offset < bytes.length) {
      const read = readSync(fd, bytes, offset, bytes.length - offset, offset);
      if (!read) break;
      offset += read;
    }
    if (offset !== stat.size) throw new Error('Input changed during bounded read');
    return JSON.parse(bytes.subarray(0, offset).toString('utf8'));
  } finally { closeSync(fd); }
}

export function writeCoverage(siteRoot) {
  const site = resolve(siteRoot);
  const report = buildCoverage(boundedJson(join(site, 'public/public-snapshot.json'), 32 * 1024 * 1024),
    boundedJson(join(site, 'geography/colorado-jurisdictions.json'), 16 * 1024 * 1024));
  for (const path of [site, join(site, 'api'), join(site, 'api/v1')]) {
    mkdirSync(path, { recursive: true });
    if (!lstatSync(path).isDirectory() || lstatSync(path).isSymbolicLink()) throw new Error('API directory is not a real directory');
  }
  const output = `${JSON.stringify(report)}\n`;
  if (Buffer.byteLength(output) > 4 * 1024 * 1024) throw new Error('Coverage output exceeds byte limit');
  const temporary = join(site, `api/v1/.coverage-${randomUUID()}.tmp`);
  try {
    writeFileSync(temporary, output, { flag: 'wx', mode: 0o644 });
    renameSync(temporary, join(site, 'api/v1/coverage.json'));
  } finally { try { unlinkSync(temporary); } catch (error) { if (error.code !== 'ENOENT') throw error; } }
  return report;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  if (process.argv.length !== 3) throw new Error('Usage: node build-coverage-api.mjs <static-site-directory>');
  const report = writeCoverage(process.argv[2]);
  console.log(`Coverage: ${report.totals.published_decisions} decisions / ${report.totals.jurisdictions_with_published_records} jurisdictions; ${report.totals.jurisdictions_without_indexed_records} roster entries without indexed records.`);
}
