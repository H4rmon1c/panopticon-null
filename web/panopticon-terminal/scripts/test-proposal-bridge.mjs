import test from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { validateSnapshot } from "../api.js";
import { buildCoverage } from "./build-coverage-api.mjs";
import { buildAlertApi } from "./build-alert-api.mjs";

const root = fileURLToPath(new URL("../../../", import.meta.url));
const temporary = mkdtempSync(join(tmpdir(), "pnull-proposal-bridge-"));
let generated;
try {
  const output = join(temporary, "generated.json");
  const run = spawnSync("cargo", ["test", "--locked", "-q", "-p", "pnull-cli",
    "case_proposal::release::tests::checked_document_proposals_export_for_browser", "--", "--exact"], {
    cwd: root, env: { ...process.env, PNUL_DOCUMENT_BRIDGE_TEST_OUTPUT: output },
    encoding: "utf8", timeout: 600000, maxBuffer: 2 * 1024 * 1024,
  });
  assert.equal(run.status, 0, `${run.error ?? ""}\n${run.stdout}\n${run.stderr}`);
  generated = JSON.parse(readFileSync(output, "utf8"));
} finally { rmSync(temporary, { recursive: true, force: true }); }
const geography = JSON.parse(readFileSync(new URL("../geography/colorado-jurisdictions.json", import.meta.url), "utf8"));
const released = JSON.parse(readFileSync(new URL("../public/public-snapshot.json", import.meta.url), "utf8"));

function parts(snapshot) {
  const decision = snapshot.entities.find(item => item.document_vote_context);
  const vote = decision.votes[0];
  const proof = snapshot.evidence.find(item => item.id === vote.evidence_id);
  const voter = snapshot.entities.find(item => item.id === vote.voter_entity_id);
  return { decision, vote, proof, voter };
}

test("actual Rust proposals with named votes pass standalone, combined coverage, and alerts", () => {
  for (const snapshot of generated.proposals) {
    validateSnapshot(snapshot);
    const coverage = buildCoverage(snapshot, geography);
    assert.equal(coverage.totals.published_decisions, 1);
    assert.equal(coverage.totals.direct_vote_rows, 2);
    assert.equal(coverage.jurisdictions.find(row => row.id === "us-co-place-0851250").decision_ids.length, 1);
    const { decision } = parts(snapshot);
    assert.match(decision.summary, /Award authorization is cited/);
    assert.doesNotMatch(decision.summary, /Contract award link not established/);
  }
  const combined = generated.combined;
  validateSnapshot(combined);
  const coverage = buildCoverage(combined, geography);
  assert.equal(coverage.totals.published_decisions, released.case_releases.length + 2);
  const moffat = coverage.jurisdictions.find(row => row.id === "us-co-place-0851250");
  assert.equal(moffat.decision_ids.length, 2);
  assert.equal(moffat.direct_vote_rows, 4);
  assert.equal(combined.sources.length, released.sources.length + 1, "shared identical source is deduplicated");
  assert.equal(combined.entities.filter(item => item.type === "JURISDICTION" && item.jurisdiction_ref === moffat.id).length, 1);
  const first = buildAlertApi(combined, "https://panopticon.fail", { now: "2026-11-10T12:00:00.000Z" });
  const second = buildAlertApi(combined, "https://panopticon.fail", { previous: first.api, now: "2026-11-10T12:01:00.000Z" });
  const records = second.api.records.filter(item => moffat.decision_ids.includes(item.case_id));
  assert.equal(records.length, 2);
  for (const record of records) {
    assert.equal(record.revision, 1);
    assert.equal(record.vote_scope, "direct");
    assert.equal(record.jurisdictions[0].name, "Moffat town");
    assert.ok(record.evidence.some(item => /^line:/.test(item.locator)));
    const news = second.feed.items.find(item => item._panopticon.case_id === record.case_id);
    assert.doesNotMatch(news.content_text, /Unresolved:.*Named direct-item positions/);
  }
});

const attacks = [
  ["changed vote", ({ vote }) => { vote.vote = "No"; }],
  ["changed name and voter", ({ vote, voter }) => { vote.name = voter.name = "Different Person"; }],
  ["cropped negation", ({ proof }) => { proof.quote = `Not ${proof.quote}`; }],
  ["multiple voter quote", ({ proof }) => { proof.quote += "; Other Person No"; }],
  ["newline in vote", ({ proof }) => { proof.quote = "Alice Smith\nAye"; }],
  ["page-only row", ({ proof }) => { proof.locator = "page:1"; }],
  ["zero row", ({ proof }) => { proof.locator = "line:0"; }],
  ["unsafe integer row", ({ proof }) => { proof.locator = "line:9007199254740993"; }],
  ["row before motion", ({ proof }) => { proof.locator = "line:1"; }],
  ["row outside motion window", ({ proof }) => { proof.locator = "line:1000"; }],
  ["consent scope", ({ decision }) => { decision.document_vote_context.scope = "consent_motion"; }],
  ["missing motion context", ({ decision }) => { delete decision.document_vote_context.motion_evidence_id; }],
  ["wrong motion proof", ({ decision }) => { decision.document_vote_context.motion_evidence_id = decision.document_vote_context.matter_evidence_id; }],
  ["missing method", ({ proof }) => { delete proof.verification_method; }],
  ["JSON row inside document context", ({ proof }) => { proof.locator = "$[0].VoteValueName"; }],
  ["document row without context", ({ decision }) => { delete decision.document_vote_context; }],
  ["orphan marker posing as JSON row", ({ decision, proof }) => { delete decision.document_vote_context; proof.locator = "$[0].VoteValueName"; }],
  ["case-scoped voter posing as JSON row", ({ decision, proof }) => { delete decision.document_vote_context; delete proof.verification_method; proof.locator = "$[0].VoteValueName"; }],
  ["other case voter", ({ voter }) => { voter.identity_scope = "decision:co-other-case"; }],
  ["unbound voter source", ({ voter }) => { voter.source_ids = []; }],
  ["missing geography refs", ({ decision, voter }) => { delete decision.jurisdiction_ref; delete voter.jurisdiction_ref; }],
  ["changed geography refs", ({ decision, voter }) => { decision.jurisdiction_ref = voter.jurisdiction_ref = "us-co-place-0000000"; }],
  ["missing voter geography", ({ voter }) => { voter.jurisdiction_ids = []; }],
  ["different source with coherent digest", ({ proof }, snapshot) => {
    const other = snapshot.sources.find(item => item.id !== proof.source_id);
    proof.source_id = other.id; proof.sha256 = other.sha256;
  }],
];
for (const [name, mutate] of attacks) test(`generated document votes reject ${name}`, () => {
  const copy = structuredClone(generated.combined);
  mutate(parts(copy), copy);
  assert.throws(() => validateSnapshot(copy), /document vote context|direct vote inconsistent/);
});

test("existing API votes cannot acquire a document-only verification marker", () => {
  const copy = structuredClone(released);
  const decision = copy.entities.find(item => item.votes?.length);
  const proof = copy.evidence.find(item => item.id === decision.votes[0].evidence_id);
  proof.verification_method = "exact_document_line_named_vote_v1";
  assert.throws(() => validateSnapshot(copy), /direct vote inconsistent/);
});
