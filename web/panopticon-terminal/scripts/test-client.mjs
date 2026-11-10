import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { runInNewContext } from "node:vm";
import { PanopticonClient, validateSnapshot } from "../api.js";
import { buildDecisionPages } from "./build-decision-pages.mjs";

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
  assert.throws(() => validateSnapshot(unreleased), /approved or source-verified case release/);
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

test("automatic source verification binds every published decision without a fake human review", () => {
  const snapshot = publicFixture();
  const auto = { state: "SOURCE_VERIFIED", bound_digest: `sha256:${"c".repeat(64)}`, reviewed_at: null, corrections: [] };
  const approved = { state: "APPROVED", bound_digest: `sha256:${"d".repeat(64)}`, reviewed_at: "2026-11-09T00:00:00Z", corrections: [] };
  snapshot.release_review = clone(auto);
  for (const slug of ["co-one", "co-two"]) snapshot.entities.push({ id: `decision_${slug}`, slug, type: "DECISION", name: slug });
  snapshot.case_releases = [
    { case_id: "co-one", release: clone(approved) },
    { case_id: "co-two", release: clone(auto) },
  ];
  assert.equal(validateSnapshot(snapshot), snapshot);

  const missing = clone(snapshot);
  missing.case_releases.pop();
  assert.throws(() => validateSnapshot(missing), /published decision lacks a case release/);
  const forged = clone(snapshot);
  forged.case_releases[1].release.reviewed_at = "2026-11-09T00:00:00Z";
  assert.throws(() => validateSnapshot(forged), /does not match a verified decision/);
  const inconsistent = clone(snapshot);
  inconsistent.release_review.state = "APPROVED";
  inconsistent.release_review.reviewed_at = "2026-11-09T00:00:00Z";
  assert.throws(() => validateSnapshot(inconsistent), /Aggregate case release state is inconsistent/);
});

test("consent agenda context never becomes an item-specific voter list", async () => {
  const snapshot = publicFixture();
  const sourceId = snapshot.sources[0].id;
  const evidenceFor = (id, claim, locator) => {
    const record = clone(snapshot.evidence[0]);
    Object.assign(record, { id, claim, locator });
    snapshot.evidence.push(record);
    return id;
  };
  const motionId = evidenceFor("consent_motion_id", "38020", "$.EventItems[13].EventItemId");
  const titleId = evidenceFor("consent_motion_title", "Approve consent agenda", "$.EventItems[13].EventItemTitle");
  const actionId = evidenceFor("consent_motion_action", "Motion to approve consent agenda", "$.EventItems[13].EventItemActionText");
  const outcomeId = evidenceFor("consent_motion_outcome", "Pass", "$.EventItems[13].EventItemPassedFlagName");
  const itemId = evidenceFor("consent_item_flag", "1", "$.EventItems[19].EventItemConsent");
  const record = {
    id: "decision_consent_test", slug: "co-consent-test", type: "DECISION", name: "Consent item",
    votes: [], vote_record_state: "consent_block", source_ids: [sourceId],
    consent_block: {
      motion_item_id: "38020", motion_title: "Approve consent agenda", motion_outcome: "Pass",
      block_vote_source_id: sourceId, motion_evidence_id: motionId,
      motion_title_evidence_id: titleId, motion_action_evidence_id: actionId,
      motion_outcome_evidence_id: outcomeId,
      item_consent_evidence_id: itemId, note: "Block vote covers multiple items",
    },
  };
  snapshot.entities.push(record);
  assert.equal(validateSnapshot(snapshot), snapshot);
  globalThis.location = { pathname: "/decision.html", search: "" };
  globalThis.document = { querySelector: () => null };
  const { buildDecisionModel } = await import("../decision.js");
  const model = buildDecisionModel(snapshot, record.slug);
  assert.equal(model.votes.length, 0);
  assert.ok(model.evidence.some((item) => item.id === outcomeId));
  const forgedVotes = clone(snapshot);
  forgedVotes.entities.at(-1).votes.push({ name: "Council member", vote: "Aye", evidence_id: outcomeId });
  assert.throws(() => validateSnapshot(forgedVotes), /invalid consent-block record/);
  const missingProof = clone(snapshot);
  missingProof.entities.at(-1).consent_block.motion_evidence_id = "missing";
  assert.throws(() => validateSnapshot(missingProof), /invalid consent-block record/);
  const swappedOutcome = clone(snapshot);
  swappedOutcome.entities.at(-1).consent_block.motion_outcome_evidence_id = titleId;
  assert.throws(() => validateSnapshot(swappedOutcome), /invalid consent-block record/);
});

test("case-specific static pages use their own metadata and escape source text", async () => {
  const snapshot = publicFixture();
  const auto = { state: "SOURCE_VERIFIED", bound_digest: `sha256:${"c".repeat(64)}`, reviewed_at: null, corrections: [] };
  snapshot.release_review = clone(auto);
  snapshot.entities.push(
    { id: "decision_legacy", slug: "co-colorado-springs-25-581-2025-11-25", type: "DECISION", name: "Colorado Springs case", share_url: "https://panopticon.fail/decision.html?decision=co-colorado-springs-25-581-2025-11-25" },
    { id: "decision_new", slug: "co-commerce-city-ord-2755-2026-04-06", type: "DECISION", name: 'Commerce City <Flock> "agreement"', summary: 'Council approved <agreement> & asked "what next?" $& $\' $`', share_url: "https://panopticon.fail/decisions/co-commerce-city-ord-2755-2026-04-06.html" },
  );
  snapshot.case_releases = [
    { case_id: snapshot.entities.at(-2).slug, release: clone(auto) },
    { case_id: snapshot.entities.at(-1).slug, release: clone(auto) },
  ];
  const template = await readFile(join(root, "decision.html"), "utf8");
  const pages = buildDecisionPages(snapshot, template, "https://panopticon.fail");
  assert.equal(pages.length, 1);
  assert.equal(pages[0].path, "decisions/co-commerce-city-ord-2755-2026-04-06.html");
  assert.match(pages[0].html, /Commerce City &lt;Flock&gt; &quot;agreement&quot;/);
  assert.match(pages[0].html, /Council approved &lt;agreement&gt; &amp; asked &quot;what next\?&quot;/);
  assert.match(pages[0].html, /&quot;what next\?&quot; \$&amp; \$&#39; \$`/);
  assert.match(pages[0].html, /href="\.\.\/decision\.css"/);
  assert.doesNotMatch(pages[0].html, /Colorado Springs police technology surcharge vote/);
  const bad = clone(snapshot);
  bad.entities.at(-1).share_url = "https://panopticon.fail/decision.html?decision=co-colorado-springs-25-581-2025-11-25";
  assert.throws(() => buildDecisionPages(bad, template, "https://panopticon.fail"), /lacks its case-specific share URL/);
});

test("atlas dossier routes published cases to their own static pages", async () => {
  const dossier = await readFile(join(root, "scripts/03-dossier.js"), "utf8");
  const entity = { slug: "co-commerce-city-ord-2755-2026-04-06", id: "decision_commerce", share_url: "https://panopticon.fail/decisions/co-commerce-city-ord-2755-2026-04-06.html" };
  const href = (mode) => runInNewContext(`${dossier}\ndossierDecisionHref(${JSON.stringify(entity)}, ${JSON.stringify(mode)})`);
  assert.equal(href("public"), "./decisions/co-commerce-city-ord-2755-2026-04-06.html");
  assert.equal(href("preview"), "./decision.html?decision=co-commerce-city-ord-2755-2026-04-06&preview=1");
  assert.equal(href("demo"), "./decision.html?decision=co-commerce-city-ord-2755-2026-04-06&demo=1");
});

test("record counts cannot carry markup into search or relationship cards", async () => {
  const payload = '<img src=x onerror="alert(1)">';
  const malformed = publicFixture();
  malformed.entities[0].source_count = payload;
  assert.throws(() => validateSnapshot(malformed), /invalid count/);
  const malformedRelation = publicFixture();
  malformedRelation.entities[0].relationships[0].source_count = payload;
  assert.throws(() => validateSnapshot(malformedRelation), /invalid relationship count/);
  const helpers = await readFile(join(root, "scripts/12-helpers.js"), "utf8");
  const search = await readFile(join(root, "scripts/05-view-search.js"), "utf8");
  const dossier = await readFile(join(root, "scripts/03-dossier.js"), "utf8");
  const list = () => ({ children: [], replaceChildren() { this.children = []; }, append(child) { this.children.push(child); } });
  const searchList = list();
  const relationList = list();
  const nodes = new Map([
    ["#search-results", searchList], ["#search-result-count", {}],
    ["#relation-list", relationList], ["#relation-count", {}],
  ]);
  const context = {
    state: {
      searchResults: [{ id: "record", type: "DECISION", name: "Record", source_count: payload }],
      searchCursor: 0,
      byId: new Map([["target", { name: "Target" }]]),
    },
    $: (selector) => nodes.get(selector),
    document: { createElement: () => ({ addEventListener() {} }) },
  };
  runInNewContext(`${helpers}\n${search}\n${dossier}\nrenderSearch();`, context);
  runInNewContext(`renderRelations({ relationships: [{ target_entity_id: "target", type: "CITES", label: "Source", source_count: ${JSON.stringify(payload)} }] });`, context);
  for (const card of [searchList.children[0], relationList.children[0]]) {
    assert.ok(card, "expected a rendered card");
    assert.doesNotMatch(card.innerHTML, /<img/i);
    assert.match(card.innerHTML, /&lt;img/);
  }
});

test("decision model keeps ordinance votes separate from an unestablished contract", async () => {
  globalThis.location = { search: "" };
  globalThis.document = { querySelector: () => null };
  const { buildDecisionModel, decisionIdFromLocation, decisionModeFromLocation } = await import("../decision.js");
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
  assert.equal(decisionIdFromLocation("/decisions/co-commerce-city-ord-2755-2026-04-06.html", "?decision=co-denver-25-0413-2025-05-05"), "co-commerce-city-ord-2755-2026-04-06");
  assert.equal(decisionIdFromLocation("/decision.html", "?decision=co-colorado-springs-25-581-2025-11-25"), "co-colorado-springs-25-581-2025-11-25");
  assert.deepEqual(decisionModeFromLocation("/decisions/co-commerce-city-ord-2755-2026-04-06.html", "?demo=1&preview=1"), { demo: false, preview: false });
  assert.deepEqual(decisionModeFromLocation("/decision.html", "?demo=1"), { demo: true, preview: false });
});
