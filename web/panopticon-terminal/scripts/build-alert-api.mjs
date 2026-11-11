import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { lstat, mkdir, open, writeFile } from "node:fs/promises";
import { isIP } from "node:net";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { validateSnapshot } from "../api.js";

export const MAX_API_BYTES = 32 * 1024 * 1024;
export const MAX_API_RECORDS = 10000;
const schema = "pnull-alerts-v1";
const digestPattern = /^sha256:[a-f0-9]{64}$/;
const slugPattern = /^co-[a-z0-9-]{1,180}$/;
const textControl = /[\u0000-\u001f\u007f-\u009f\u202a-\u202e\u2066-\u2069]/u;
const notice = "Publication time is not source freshness. Check /api/v1/health.json before automation. This feed contains current released records and withdrawal notices, not a complete history or statewide coverage claim.";

function text(value, label, max = 4096) {
  if (typeof value !== "string" || !value.length || value.length > max || textControl.test(value)) throw new Error(`Invalid ${label}`);
  return value;
}
function list(value, label, max = 256) {
  if (!Array.isArray(value) || value.length > max) throw new Error(`Invalid or oversized ${label}`);
  return value;
}
function timestamp(value) {
  if (typeof value !== "string" || !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/.test(value) || new Date(value).toISOString() !== value) throw new Error("Invalid API timestamp");
  return value;
}
function publicUrl(value) {
  text(value, "public URL", 8192);
  const url = new URL(value);
  if (url.protocol !== "https:" || url.username || url.password || isIP(url.hostname.replace(/^\[|\]$/g, "")) || !url.hostname.includes(".") || /\.(localhost|local|internal|invalid|test|example)$/.test(url.hostname)) throw new Error("Invalid public HTTPS URL");
  return url;
}
function originOf(siteUrl) {
  const url = publicUrl(siteUrl);
  if (url.href !== `${url.origin}/`) throw new Error("Site URL must be an HTTPS origin");
  return url.origin;
}
function digest(value) {
  return `sha256:${createHash("sha256").update(JSON.stringify(value)).digest("hex")}`;
}
function bound(value) {
  if (!digestPattern.test(value)) throw new Error("Missing release digest");
  return value;
}
function slug(value) {
  if (typeof value !== "string" || !slugPattern.test(value)) throw new Error("Unsafe case ID");
  return value;
}

function activePayload(decision, release, { entities, sources, evidence }, origin) {
  const caseId = slug(decision.slug);
  const url = `${origin}/decisions/${caseId}.html`;
  if (decision.share_url !== url) throw new Error("Decision share URL does not match canonical origin");
  const proofIds = new Set();
  for (const item of [...list(decision.attributes ?? [], "attributes"), ...list(decision.timeline ?? [], "timeline"), ...list(decision.relationships ?? [], "relationships"), ...list(decision.votes ?? [], "votes"), ...list(decision.technology_context ?? [], "technology context")]) {
    if (item.evidence_id) proofIds.add(item.evidence_id);
  }
  for (const [key, value] of Object.entries(decision.consent_block ?? {})) if (key.endsWith("_evidence_id")) proofIds.add(value);
  for (const item of list(decision.consent_block?.roll_call ?? [], "consent roll call")) proofIds.add(item.evidence_id);
  if (decision.vote_gap?.evidence_id) proofIds.add(decision.vote_gap.evidence_id);
  if (proofIds.size > 256) throw new Error("Too many evidence references");
  const proof = [...proofIds].sort().map((id) => {
    const record = evidence.get(id);
    const source = sources.get(record?.source_id);
    if (!record || !source) throw new Error("Missing released evidence source");
    publicUrl(source.canonical_url);
    return { source_url: source.canonical_url, source_sha256: source.sha256, locator: text(record.locator, "evidence locator"), verification_digest: bound(record.verification_digest) };
  });
  const sourceLinks = list(decision.source_ids ?? [], "source IDs", 64).map((id) => {
    const source = sources.get(id);
    if (!source) throw new Error("Missing decision source");
    publicUrl(source.canonical_url);
    return { url: source.canonical_url, sha256: source.sha256 };
  }).sort((a, b) => a.url.localeCompare(b.url));
  const jurisdictions = list(decision.jurisdiction_ids ?? [], "jurisdictions", 32).map((id) => {
    const item = entities.get(id);
    if (item?.type !== "JURISDICTION") throw new Error("Unknown decision jurisdiction");
    return { id: text(id, "jurisdiction ID", 256), name: text(item.name, "jurisdiction name", 512) };
  }).sort((a, b) => a.id.localeCompare(b.id));
  const outcome = (decision.attributes ?? []).find((item) => ["Outcome", "Recorded outcome"].includes(item.label));
  const date = decision.updated_at;
  if (typeof date !== "string" || !/^\d{4}-\d{2}-\d{2}$/.test(date) || new Date(`${date}T00:00:00Z`).toISOString().slice(0, 10) !== date) throw new Error("Invalid recorded decision date");
  const unknowns = list(decision.unknowns ?? [], "unknowns", 64).map((item) => ({ kind: text(item.kind, "gap kind", 256), state: text(item.state, "gap state", 64), detail: text(item.detail, "gap detail") }));
  return {
    case_id: caseId, state: "active", url,
    title: text(decision.social_title ?? decision.name, "title", 1024),
    summary: text(decision.social_summary ?? decision.summary ?? decision.description, "summary", 8192),
    decision_date: date, recorded_outcome: outcome ? text(outcome.value, "recorded outcome", 512) : "not_established",
    vote_scope: decision.vote_record_state ?? ((decision.votes ?? []).length ? "direct" : "not_established"),
    jurisdictions, unknowns, unresolved_question: decision.unresolved_question === undefined ? null : text(decision.unresolved_question, "unresolved question"),
    source_links: sourceLinks, evidence: proof, release_digest: bound(release.bound_digest),
    verification_state: release.state,
    source_last_checked_at: null,
  };
}

function withdrawnPayload(caseId, releaseDigest, origin) {
  return { case_id: slug(caseId), state: "withdrawn", url: `${origin}/decisions/${caseId}.html`,
    title: "Record withdrawn pending correction", summary: "This record is withdrawn. Previous claims must not be treated as current findings.",
    decision_date: null, recorded_outcome: "not_established", vote_scope: "not_established", jurisdictions: [], unknowns: [], unresolved_question: null,
    source_links: [], evidence: [], release_digest: bound(releaseDigest), verification_state: "WITHDRAWN", source_last_checked_at: null };
}

const payloadKeys = Object.keys(withdrawnPayload("co-example", `sha256:${"a".repeat(64)}`, "https://panopticon.fail"));
const metaKeys = ["content_digest", "revision", "revision_id", "change", "first_published_at", "changed_at"];
function priorRecords(previous, origin, now) {
  if (previous === null) return new Map();
  if (previous?.schema_version !== schema || previous.site_url !== origin || timestamp(previous.generated_at) > now) throw new Error("Invalid previous API release");
  const result = new Map();
  for (const record of list(previous.records, "previous records", MAX_API_RECORDS)) {
    const caseId = slug(record.case_id);
    if (result.has(caseId) || Object.keys(record).some((key) => ![...payloadKeys, ...metaKeys].includes(key)) || Object.keys(record).length !== payloadKeys.length + metaKeys.length) throw new Error("Duplicate or unexpected previous record fields");
    const payload = Object.fromEntries(payloadKeys.map((key) => [key, record[key]]));
    if (!["active", "withdrawn"].includes(record.state) || record.url !== `${origin}/decisions/${caseId}.html` || !Number.isSafeInteger(record.revision) || record.revision < 1 || record.revision >= Number.MAX_SAFE_INTEGER || record.content_digest !== digest(payload) || record.revision_id !== digest([caseId, record.revision, record.content_digest])) throw new Error("Invalid previous record revision");
    if (timestamp(record.first_published_at) > timestamp(record.changed_at) || record.changed_at > previous.generated_at || !["published", "updated", "reinstated", "withdrawn"].includes(record.change)) throw new Error("Invalid previous change metadata");
    if (record.state === "withdrawn" && JSON.stringify(payload) !== JSON.stringify(withdrawnPayload(caseId, record.release_digest, origin))) throw new Error("Previous withdrawal carries unsupported content");
    result.set(caseId, record);
  }
  return result;
}

export function buildAlertApi(snapshot, siteUrl, { previous = null, now = new Date().toISOString() } = {}) {
  validateSnapshot(snapshot);
  if (!Array.isArray(snapshot.case_releases)) throw new Error("Alerts require per-case release records");
  const origin = originOf(siteUrl);
  timestamp(now);
  const prior = priorRecords(previous, origin, now);
  const releases = new Map(snapshot.case_releases.map((item) => [item.case_id, item.release]));
  const indices = {
    entities: new Map(snapshot.entities.map((item) => [item.id, item])),
    sources: new Map(snapshot.sources.map((item) => [item.id, item])),
    evidence: new Map(snapshot.evidence.map((item) => [item.id, item])),
  };
  const decisions = snapshot.entities.filter((item) => item.type === "DECISION");
  if (decisions.length + (snapshot.withdrawals?.length ?? 0) > MAX_API_RECORDS) throw new Error("API record limit exceeded; shard before growing");
  const current = new Map();
  for (const decision of decisions) {
    if (current.has(decision.slug)) throw new Error("Duplicate case slug");
    current.set(decision.slug, activePayload(decision, releases.get(decision.slug), indices, origin));
  }
  for (const item of snapshot.withdrawals ?? []) current.set(item.case_id, withdrawnPayload(item.case_id, item.bound_digest, origin));
  for (const [caseId, old] of prior) {
    if (current.has(caseId)) continue;
    if (old.state !== "withdrawn") throw new Error(`Active case ${caseId} disappeared without an explicit withdrawal`);
    current.set(caseId, withdrawnPayload(caseId, old.release_digest, origin));
  }
  if (current.size > MAX_API_RECORDS) throw new Error("API record limit exceeded; shard before growing");
  const records = [...current.values()].map((payload) => {
    const old = prior.get(payload.case_id);
    const contentDigest = digest(payload);
    const unchanged = old?.content_digest === contentDigest;
    const revision = old ? old.revision + (unchanged ? 0 : 1) : 1;
    return { ...payload, content_digest: contentDigest, revision, revision_id: digest([payload.case_id, revision, contentDigest]),
      change: unchanged ? old.change : payload.state === "withdrawn" ? "withdrawn" : !old ? "published" : old.state === "withdrawn" ? "reinstated" : "updated",
      first_published_at: old?.first_published_at ?? now, changed_at: unchanged ? old.changed_at : now };
  }).sort((a, b) => b.changed_at.localeCompare(a.changed_at) || a.case_id.localeCompare(b.case_id));
  const api = { schema_version: schema, site_url: origin, generated_at: now, notice, coverage_url: `${origin}/api/v1/coverage.json`, health_url: `${origin}/api/v1/health.json`, records };
  const feed = { version: "https://jsonfeed.org/version/1.1", title: "Panopticon Colorado public records", home_page_url: origin, feed_url: `${origin}/api/v1/feed.json`, description: notice,
    items: records.map((record) => {
      const unresolved = record.unknowns.filter((item) => item.state === "not_established");
      return { id: record.url, url: record.url, title: record.title, content_text: `${record.summary}${unresolved.length ? `\nUnresolved: ${unresolved.map((item) => item.detail).join(" ")}` : ""}`, date_published: record.first_published_at, date_modified: record.changed_at,
        _panopticon: { case_id: record.case_id, revision_id: record.revision_id, revision: record.revision, state: record.state, change: record.change, decision_date: record.decision_date, source_last_checked_at: null } };
    }) };
  serializeApi(api); serializeApi(feed);
  return { api, feed };
}

export function serializeApi(value) {
  const result = `${JSON.stringify(value, null, 2).replace(/</g, "\\u003c").replace(/>/g, "\\u003e").replace(/&/g, "\\u0026")}\n`;
  if (Buffer.byteLength(result) > MAX_API_BYTES) throw new Error("API byte limit exceeded; shard before growing");
  return result;
}
async function readBounded(path) {
  let file;
  try { file = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW); }
  catch (error) {
    if (error.code === "ELOOP") throw new Error("API input must be a bounded regular file");
    throw error;
  }
  try {
    const info = await file.stat();
    if (!info.isFile() || info.size > MAX_API_BYTES) throw new Error("API input must be a bounded regular file");
    const body = Buffer.alloc(info.size + 1);
    let offset = 0;
    while (offset < body.length) {
      const { bytesRead } = await file.read(body, offset, body.length - offset, offset);
      if (!bytesRead) break;
      offset += bytesRead;
    }
    if (offset !== info.size) throw new Error("API input changed during bounded read");
    return JSON.parse(body.subarray(0, offset).toString("utf8"));
  } finally { await file.close(); }
}
async function realDirectory(path) {
  await mkdir(path, { recursive: true });
  const info = await lstat(path);
  if (!info.isDirectory() || info.isSymbolicLink()) throw new Error("API output directory must be a real directory");
}
async function writeRetainedWithdrawalPages(stage, api) {
  await realDirectory(join(stage, "decisions"));
  for (const record of api.records.filter((item) => item.state === "withdrawn")) {
    const path = join(stage, `decisions/${record.case_id}.html`);
    try {
      const info = await lstat(path);
      if (!info.isFile() || info.isSymbolicLink()) throw new Error("Withdrawal page must be a regular file");
      continue; // The normal decision-page generator already emitted this notice.
    } catch (error) { if (error.code !== "ENOENT") throw error; }
    const url = record.url.replace(/[&<>"']/g, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[character]);
    await writeFile(path, `<!doctype html>\n<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="robots" content="noindex,follow"><link rel="canonical" href="${url}"><title>Record withdrawn pending correction | PANOPTICON.FAIL</title></head><body><main><h1>Record withdrawn pending correction</h1><p>This record is withdrawn. Previous claims must not be treated as current findings.</p><p><a href="../index.html">Return to the Colorado atlas</a></p></main></body></html>\n`, { flag: "wx" });
  }
}
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [, , stage, siteUrl, previousPath, ...extra] = process.argv;
  if (!stage || !siteUrl || extra.length) throw new Error("Usage: build-alert-api.mjs STAGE SITE_ORIGIN [PREVIOUS_ALERTS_JSON]");
  const snapshot = await readBounded(join(stage, "public/public-snapshot.json"));
  const previous = previousPath ? await readBounded(previousPath) : null;
  const { api, feed } = buildAlertApi(snapshot, siteUrl, { previous });
  for (const path of [stage, join(stage, "api"), join(stage, "api/v1")]) await realDirectory(path);
  await writeFile(join(stage, "api/v1/alerts.json"), serializeApi(api), { flag: "wx" });
  await writeFile(join(stage, "api/v1/feed.json"), serializeApi(feed), { flag: "wx" });
  await writeRetainedWithdrawalPages(stage, api);
  console.log(`generated alerts API with ${api.records.length} current records/withdrawals`);
}
