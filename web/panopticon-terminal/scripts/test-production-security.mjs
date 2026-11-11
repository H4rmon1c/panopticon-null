import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { PanopticonClient, validateSnapshot } from "../api.js";

const released = JSON.parse(await readFile(new URL("../public/public-snapshot.json", import.meta.url), "utf8"));
const copy = () => structuredClone(released);

test("direct roll calls reject changed vote values and unrelated evidence", () => {
  for (const slug of ["co-colorado-springs", "co-commerce-city"]) {
    const snapshot = copy();
    const decision = snapshot.entities.find((item) => item.slug?.startsWith(slug));
    const vote = decision.votes[0];
    vote.vote = vote.vote === "Aye" ? "Nay" : "Aye";
    assert.throws(() => validateSnapshot(snapshot), /direct vote inconsistent/);
    const unrelated = copy();
    const row = unrelated.entities.find((item) => item.slug?.startsWith(slug)).votes[0];
    row.evidence_id = unrelated.evidence.find((item) => !item.locator.includes("VoteValueName")).id;
    assert.throws(() => validateSnapshot(unrelated), /direct vote inconsistent/);
  }
});

test("initial snapshot and update checks share in-flight requests", async () => {
  const original = globalThis.fetch;
  let calls = 0;
  globalThis.fetch = async () => { calls++; return { ok: true, json: async () => copy() }; };
  try {
    const client = new PanopticonClient();
    await Promise.all([client.bootstrap(), client.search("")]);
    assert.equal(calls, 1);
    assert.deepEqual(await Promise.all([client.checkForUpdates(), client.checkForUpdates()]), [false, false]);
    assert.equal(calls, 2);
  } finally { globalThis.fetch = original; }
});

test("update detection handles withdrawals without mixing hydrated versions", async () => {
  const original = globalThis.fetch;
  let next = copy();
  globalThis.fetch = async () => ({ ok: true, json: async () => structuredClone(next) });
  try {
    const client = new PanopticonClient();
    await client.bootstrap();
    const initial = client.snapshot;
    next.status.last_publish = "2099-01-01T00:00:00Z";
    assert.equal(await client.checkForUpdates(), false, "export clock alone is not a content update");
    next.withdrawals = [{ case_id: "co-previous-case", state: "withdrawn", bound_digest: `sha256:${"a".repeat(64)}`, decided_at: "2026-11-10T12:00:00Z", review_id: "withdrawal-review" }];
    assert.equal(await client.checkForUpdates(), true);
    assert.equal(client.snapshot, initial, "loaded entity/evidence references remain one version until reload");
    next.preview = true;
    await assert.rejects(client.checkForUpdates(), /Preview and published/);
    assert.equal(client.snapshot, initial, "invalid replacement cannot enter public UI");
    next = copy();
    assert.equal(await client.checkForUpdates(), false, "failed checks can retry");
  } finally { globalThis.fetch = original; }
});
