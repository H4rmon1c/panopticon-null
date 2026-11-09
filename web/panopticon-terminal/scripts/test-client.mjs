import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { PanopticonClient, validateSnapshot } from "../api.js";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const demo = JSON.parse(await readFile(join(root, "mock/public-snapshot.json"), "utf8"));
const clone = (value) => structuredClone(value);

function publicFixture() {
  const snapshot = clone(demo);
  snapshot.demo = false;
  snapshot.status.state = "released";
  snapshot.release_review = { state: "APPROVED", bound_digest: `sha256:${"a".repeat(64)}`, reviewed_at: "2026-11-09T00:00:00Z" };
  for (const source of snapshot.sources) delete source.demo;
  for (const record of snapshot.evidence) { delete record.demo; record.review_state = "SOURCE_VERIFIED"; }
  for (const entity of snapshot.entities) {
    entity.tags = (entity.tags ?? []).filter((tag) => tag !== "DEMO DATA");
    for (const item of [...(entity.relationships ?? []), ...(entity.attributes ?? []), ...(entity.timeline ?? []), ...(entity.votes ?? []), ...(entity.technology_context ?? [])]) item.review_state = "SOURCE_VERIFIED";
  }
  return snapshot;
}

function previewFixture() {
  const snapshot = publicFixture();
  snapshot.preview = true;
  snapshot.status.state = "pending_release";
  snapshot.release_review = { state: "PENDING_RELEASE", bound_digest: `sha256:${"b".repeat(64)}`, reviewed_at: null };
  for (const record of snapshot.evidence) record.review_state = "PENDING_RELEASE";
  for (const entity of snapshot.entities) {
    for (const item of [...(entity.relationships ?? []), ...(entity.attributes ?? []), ...(entity.timeline ?? []), ...(entity.votes ?? []), ...(entity.technology_context ?? [])]) item.review_state = "PENDING_RELEASE";
  }
  return snapshot;
}

test("default client loads only the case-released static snapshot", async () => {
  const originalFetch = globalThis.fetch;
  const requested = [];
  globalThis.fetch = async (url) => {
    requested.push(url);
    return { ok: true, json: async () => publicFixture() };
  };
  try {
    const client = new PanopticonClient();
    const result = await client.bootstrap();
    assert.equal(result.mode, "public");
    assert.deepEqual(requested, ["./public/public-snapshot.json"]);
    assert.equal((await client.search(""))[0].kind, "entity");
    assert.equal(requested.length, 1);
  } finally { globalThis.fetch = originalFetch; }
});

test("missing public snapshot never falls back to synthetic data", async () => {
  const originalFetch = globalThis.fetch;
  const requested = [];
  globalThis.fetch = async (url) => {
    requested.push(url);
    return { ok: false, status: 404 };
  };
  try {
    await assert.rejects(new PanopticonClient().bootstrap(), /released public snapshot \(404\)/);
    assert.deepEqual(requested, ["./public/public-snapshot.json"]);
  } finally { globalThis.fetch = originalFetch; }
});

test("demonstration and private preview require explicit modes", async () => {
  const originalFetch = globalThis.fetch;
  const requested = [];
  globalThis.fetch = async (url) => {
    requested.push(url);
    const snapshot = url.includes("preview") ? previewFixture() : clone(demo);
    return { ok: true, json: async () => snapshot };
  };
  try {
    assert.equal((await new PanopticonClient({ demo: true }).bootstrap()).mode, "demo");
    assert.equal((await new PanopticonClient({ preview: true }).bootstrap()).mode, "preview");
    assert.deepEqual(requested, ["./mock/public-snapshot.json", "./preview-snapshot.json"]);
    assert.throws(() => new PanopticonClient({ demo: true, preview: true }), /either/);
  } finally { globalThis.fetch = originalFetch; }
});

test("published data requires case release, source verification, and supported links", () => {
  const pending = previewFixture();
  assert.throws(() => validateSnapshot(pending), /Preview and published datasets/);
  const unreleased = publicFixture();
  unreleased.release_review.state = "PENDING_RELEASE";
  assert.throws(() => validateSnapshot(unreleased), /approved case release/);
  const unverified = publicFixture();
  unverified.evidence[0].review_state = "PENDING_RELEASE";
  assert.throws(() => validateSnapshot(unverified), /source-verification state/);
  const missingBinding = publicFixture();
  missingBinding.evidence[0].verification_digest = "";
  assert.throws(() => validateSnapshot(missingBinding), /verification digest/);
  const mixed = publicFixture();
  mixed.sources[0].demo = true;
  assert.throws(() => validateSnapshot(mixed), /demonstration boundary/);
  const unsupported = publicFixture();
  unsupported.entities[0].relationships[0].evidence_id = "missing";
  assert.throws(() => validateSnapshot(unsupported), /unsupported relationship/);
  const vote = publicFixture();
  vote.entities[0].votes = [{ name: "Test", vote: "Yes", evidence_id: "missing" }];
  assert.throws(() => validateSnapshot(vote), /vote without evidence/);
});

test("decision model keeps ordinance votes separate from an unestablished contract", async () => {
  globalThis.location = { search: "" };
  globalThis.document = { querySelector: () => null };
  const { buildDecisionModel } = await import("../decision.js");
  const snapshot = publicFixture();
  snapshot.entities.push({
    id: "decision_co_colorado_springs_25_581_2025_11_25",
    slug: "co-colorado-springs-25-581-2025-11-25",
    type: "DECISION",
    name: "Ordinance 25-581",
    source_ids: [snapshot.sources[0].id],
    relationships: [],
    attributes: [],
    timeline: [],
    votes: [{ name: "Public council member", vote: "Yes", evidence_id: snapshot.evidence[0].id }],
    unknowns: [{ kind: "contract_award", state: "not_established", detail: "A vendor contract is not established by this vote." }],
  });
  const model = buildDecisionModel(snapshot, "co-colorado-springs-25-581-2025-11-25");
  assert.equal(model.votes.length, 1);
  assert.equal(model.contracts.length, 0);
  assert.match(model.unknowns[0], /vendor contract/);
  assert.equal(model.evidence[0].id, snapshot.evidence[0].id);
});
