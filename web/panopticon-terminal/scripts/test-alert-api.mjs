import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { mkdtemp, mkdir, readFile, writeFile, rm, symlink, truncate } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { buildAlertApi, serializeApi, MAX_API_BYTES } from "./build-alert-api.mjs";

const original = JSON.parse(readFileSync(new URL("../public/public-snapshot.json", import.meta.url)));
const origin = "https://panopticon.fail";
const firstTime = "2026-11-10T01:00:00.000Z";
const secondTime = "2026-11-10T02:00:00.000Z";
const thirdTime = "2026-11-10T03:00:00.000Z";
const clone = () => structuredClone(original);
const initial = () => buildAlertApi(clone(), origin, { now: firstTime });
function withdraw(snapshot, id) {
  snapshot.entities = snapshot.entities.filter((item) => item.slug !== id);
  snapshot.case_releases = snapshot.case_releases.filter((item) => item.case_id !== id);
  snapshot.withdrawals = [{ case_id: id, state: "withdrawn", bound_digest: `sha256:${"c".repeat(64)}`, decided_at: secondTime, review_id: "private-review-do-not-export", note: "private reviewer note" }];
}

test("API exposes only released decisions, preserves failed outcome and unknown links", () => {
  const { api, feed } = initial();
  assert.equal(api.records.length, original.entities.filter((item) => item.type === "DECISION").length);
  const denver = api.records.find((record) => record.case_id === "co-denver-25-0413-2025-05-05");
  assert.equal(denver.recorded_outcome, "Fail");
  assert.match(denver.summary, /did not approve/);
  assert.ok(denver.unknowns.length);
  assert.ok(denver.evidence.every((proof) => proof.locator && /^[a-f0-9]{64}$/.test(proof.source_sha256)));
  assert.ok(api.records.every((record) => record.source_last_checked_at === null));
  assert.equal(api.health_url, `${origin}/api/v1/health.json`);
  assert.equal(api.coverage_url, `${origin}/api/v1/coverage.json`);
  assert.equal(feed.items.length, api.records.length);
  assert.ok(feed.items.every((item) => !Object.hasOwn(item, "content_html")));
});

test("repeated exports retain revision IDs and publication/change times", () => {
  const { api: previous } = initial();
  const { api } = buildAlertApi(clone(), origin, { previous, now: secondTime });
  assert.deepEqual(api.records, previous.records);
  assert.equal(api.generated_at, secondTime);
});

test("content changes and reversions produce distinct monotonic revision IDs", () => {
  const { api: previous } = initial();
  const snapshot = clone();
  const decision = snapshot.entities.find((item) => item.type === "DECISION");
  decision.social_summary = "A corrected public summary with its original evidence boundary.";
  const { api } = buildAlertApi(snapshot, origin, { previous, now: secondTime });
  const record = api.records.find((item) => item.case_id === decision.slug);
  assert.equal(record.change, "updated");
  assert.equal(record.revision, 2);
  assert.equal(record.first_published_at, firstTime);
  assert.equal(record.changed_at, secondTime);
  const restored = buildAlertApi(clone(), origin, { previous: api, now: thirdTime }).api.records.find((item) => item.case_id === decision.slug);
  assert.equal(restored.revision, 3);
  assert.notEqual(restored.revision_id, previous.records.find((item) => item.case_id === decision.slug).revision_id);
});

test("withdrawals remove old claims, retain tombstones, and mark reinstatement", () => {
  const { api: previous } = initial();
  const snapshot = clone();
  const id = previous.records[0].case_id;
  withdraw(snapshot, id);
  const { api } = buildAlertApi(snapshot, origin, { previous, now: secondTime });
  const notice = api.records.find((item) => item.case_id === id);
  assert.equal(notice.state, "withdrawn");
  assert.equal(notice.change, "withdrawn");
  assert.equal(notice.revision, 2);
  assert.deepEqual(notice.source_links, []);
  assert.deepEqual(notice.evidence, []);
  assert.doesNotMatch(JSON.stringify(api), /private reviewer note|private-review-do-not-export/);
  snapshot.withdrawals = [];
  const retained = buildAlertApi(snapshot, origin, { previous: api, now: thirdTime }).api;
  assert.deepEqual(retained.records.find((item) => item.case_id === id), notice);
  const reinstated = buildAlertApi(clone(), origin, { previous: retained, now: thirdTime }).api.records.find((item) => item.case_id === id);
  assert.equal(reinstated.change, "reinstated");
  assert.equal(reinstated.revision, 3);
});

test("silently disappearing active cases and duplicate case slugs fail closed", () => {
  const { api: previous } = initial();
  const snapshot = clone();
  withdraw(snapshot, previous.records[0].case_id);
  snapshot.withdrawals = [];
  assert.throws(() => buildAlertApi(snapshot, origin, { previous, now: secondTime }), /disappeared without an explicit withdrawal/);
  const duplicate = clone();
  const decision = duplicate.entities.find((item) => item.type === "DECISION");
  duplicate.entities.push({ ...decision, id: `${decision.id}_duplicate` });
  assert.throws(() => buildAlertApi(duplicate, origin), /Duplicate case slug/);
});

test("preview, demo, missing case release and wrong canonical origin are rejected", () => {
  for (const mode of ["preview", "demo"]) {
    const snapshot = clone(); snapshot[mode] = true;
    assert.throws(() => buildAlertApi(snapshot, origin));
  }
  const missing = clone(); delete missing.case_releases;
  assert.throws(() => buildAlertApi(missing, origin), /per-case release/);
  assert.throws(() => buildAlertApi(clone(), "https://elsewhere.example.org"), /canonical origin/);
  for (const badOrigin of ["http://panopticon.fail", "https://u:p@panopticon.fail", "https://panopticon.fail/path", "https://127.0.0.1", "https://localhost"]) assert.throws(() => buildAlertApi(clone(), badOrigin));
});

test("malicious markup remains inert JSON text and private extra fields stay out", () => {
  const snapshot = clone();
  snapshot.private_notes = "root-secret";
  const decision = snapshot.entities.find((item) => item.type === "DECISION");
  decision.social_summary = '<script>alert("x")</script> & <img src=x onerror=alert(1)>';
  decision.private_note = "operator-secret";
  const { api, feed } = buildAlertApi(snapshot, origin, { now: firstTime });
  for (const output of [serializeApi(api), serializeApi(feed)]) {
    assert.doesNotMatch(output, /<script>|<img|root-secret|operator-secret/);
    assert.match(output, /\\u003cscript\\u003e/);
    JSON.parse(output);
  }
});

test("source credentials, unsafe text controls and oversized fields are rejected", () => {
  const secretUrl = clone(); secretUrl.sources[0].canonical_url = "https://secret:password@city.example.org/records";
  assert.throws(() => buildAlertApi(secretUrl, origin), /public HTTPS URL/);
  for (const summary of ["spoof\u202etext", "bad\u0000text", "x".repeat(8193)]) {
    const snapshot = clone(); snapshot.entities.find((item) => item.type === "DECISION").social_summary = summary;
    assert.throws(() => buildAlertApi(snapshot, origin), /Invalid summary/);
  }
  assert.throws(() => serializeApi({ text: "x".repeat(MAX_API_BYTES) }), /byte limit/);
});

test("tampered or future previous API fails before carrying metadata forward", () => {
  const { api } = initial();
  for (const mutate of [
    (value) => { value.records.push(value.records[0]); },
    (value) => { value.records[0].revision_id = `sha256:${"f".repeat(64)}`; },
    (value) => { value.records[0].private_note = "secret"; },
    (value) => { value.records[0].summary = "forged previous contents"; },
    (value) => { value.generated_at = "2099-01-01T00:00:00.000Z"; },
    (value) => { value.records = Array(10001).fill(value.records[0]); },
  ]) {
    const previous = structuredClone(api); mutate(previous);
    assert.throws(() => buildAlertApi(clone(), origin, { previous, now: secondTime }));
  }
});

test("CLI bounds file reads and refuses symlinks and overwriting generated output", async () => {
  const dir = await mkdtemp(join(tmpdir(), "pnull-api-"));
  const script = fileURLToPath(new URL("./build-alert-api.mjs", import.meta.url));
  try {
    await mkdir(join(dir, "public"));
    const input = join(dir, "public/public-snapshot.json");
    await writeFile(input, JSON.stringify(original));
    const run = (...args) => spawnSync(process.execPath, [script, dir, origin, ...args], { encoding: "utf8" });
    assert.equal(run().status, 0);
    assert.notEqual(run().status, 0, "existing output cannot be overwritten outside staged publication");
    await rm(join(dir, "api"), { recursive: true });
    await mkdir(join(dir, "outside"));
    await symlink(join(dir, "outside"), join(dir, "api"));
    assert.match(run().stderr, /real directory/);
    await assert.rejects(readFile(join(dir, "outside/v1/alerts.json")), /ENOENT/);
    await rm(join(dir, "api"));
    await rm(input);
    await symlink(fileURLToPath(new URL("../public/public-snapshot.json", import.meta.url)), input);
    assert.match(run().stderr, /bounded regular file/);
    await rm(input);
    await writeFile(input, "");
    await truncate(input, MAX_API_BYTES + 1);
    assert.match(run().stderr, /bounded regular file/);
  } finally { await rm(dir, { recursive: true, force: true }); }
});

test("retained withdrawal alerts keep a noindex correction page in later releases", async () => {
  const dir = await mkdtemp(join(tmpdir(), "pnull-api-withdrawal-"));
  try {
    const { api: first } = initial();
    const snapshot = clone();
    const caseId = first.records[0].case_id;
    withdraw(snapshot, caseId);
    const { api: previous } = buildAlertApi(snapshot, origin, { previous: first, now: secondTime });
    snapshot.withdrawals = [];
    await mkdir(join(dir, "public"));
    await writeFile(join(dir, "public/public-snapshot.json"), JSON.stringify(snapshot));
    await writeFile(join(dir, "previous.json"), serializeApi(previous));
    const run = spawnSync(process.execPath, [fileURLToPath(new URL("./build-alert-api.mjs", import.meta.url)), dir, origin, join(dir, "previous.json")], { encoding: "utf8" });
    assert.equal(run.status, 0, run.stderr);
    const page = await readFile(join(dir, `decisions/${caseId}.html`), "utf8");
    assert.match(page, /noindex,follow/);
    assert.match(page, /Record withdrawn pending correction/);
    assert.doesNotMatch(page, /private-review|private reviewer/);
  } finally { await rm(dir, { recursive: true, force: true }); }
});
