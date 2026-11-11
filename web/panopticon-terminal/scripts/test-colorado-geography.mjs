import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { coverageDecisionIndex, coverageDecisionsFor } from "./24-colorado-coverage.mjs";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const registry = JSON.parse(readFileSync(join(root, "geography/colorado-jurisdictions.json"), "utf8"));
assert.equal(registry.schema_version, 1);
assert.equal(registry.purpose.includes("no approval"), true);
assert.deepEqual(registry.counts, {
  state: 1,
  county: 64,
  municipality: 273,
  school_district: 178,
  district_provisional: 3595,
});
assert.equal(registry.jurisdictions.length, 4111);
assert.equal(new Set(registry.jurisdictions.map((record) => record.id)).size, 4111);

const files = {
  census_counties: "counties.zip",
  census_places: "places.zip",
  census_schools: "unified_school.zip",
  dola_districts: "dola_districts.json",
};
const archiveManifest = JSON.parse(readFileSync(join(root, "geography/source-archives/manifest.json"), "utf8"));
assert.equal(archiveManifest.archived_on, "2026-11-10");
for (const [key, name] of Object.entries(files)) {
  const source = registry.sources[key];
  assert.match(source.url, /^https:\/\/(www2\.census\.gov|data\.colorado\.gov)\//);
  assert.equal(source.url, archiveManifest.files[key].url);
  assert.equal(source.archived_on, archiveManifest.archived_on);
  assert.equal(archiveManifest.files[key].file, name);
  const bytes = readFileSync(join(root, "geography/source-archives", name));
  assert.equal(createHash("sha256").update(bytes).digest("hex"), source.sha256);
  assert.equal(source.sha256, archiveManifest.files[key].sha256);
}
for (const record of registry.jurisdictions) {
  assert.ok(registry.sources[record.source_id], record.id);
  assert.ok(record.source_locator, record.id);
  assert.ok(record.name, record.id);
  assert.equal("case_status" in record, false, "Geography must not assert case status");
}
const counties = registry.jurisdictions.filter((record) => record.kind === "county");
assert.ok(counties.every((record) => /^us-co-county-08\d{3}$/.test(record.id)));
assert.ok(counties.every((record) => ["Polygon", "MultiPolygon"].includes(record.geometry.type)));
const municipalities = registry.jurisdictions.filter((record) => record.kind === "municipality");
assert.ok(municipalities.every((record) => /^us-co-place-08\d{5}$/.test(record.id)));
assert.ok(municipalities.every((record) => / (city|town)$/.test(record.name)), "CDPs must not be listed as municipalities");
assert.equal(registry.jurisdictions.filter((record) => "published_entity_id" in record).length, 1);
const springs = municipalities.find((record) => record.id === "us-co-place-0816000");
assert.equal(springs.published_entity_id, "jurisdiction_co_colorado_springs");
assert.ok(registry.jurisdictions.some((record) => record.id === "us-co-place-0816495"));
assert.ok(registry.jurisdictions.some((record) => record.id === "us-co-place-0812415"));
assert.ok(registry.jurisdictions.some((record) => record.id === "us-co-place-0820000"));

const publicSnapshot = {
  mode: "public",
  entities: [
    { id: "jurisdiction_co_colorado_springs", type: "JURISDICTION" },
    { id: "decision_springs", type: "DECISION", name: "Ordinance 25-93", slug: "co-springs", jurisdiction_ids: ["jurisdiction_co_colorado_springs"] },
    { id: "decision_elsewhere", type: "DECISION", slug: "elsewhere", jurisdiction_ids: ["other"] },
  ],
};
assert.deepEqual(coverageDecisionsFor(springs, publicSnapshot).map((decision) => decision.id), ["decision_springs"]);
assert.deepEqual(coverageDecisionsFor(springs, { ...publicSnapshot, mode: "preview" }), []);
assert.deepEqual(coverageDecisionsFor(springs, { ...publicSnapshot, mode: "demo" }), []);
assert.deepEqual(coverageDecisionsFor(springs, { mode: "public", entities: publicSnapshot.entities.slice(1) }), []);
assert.deepEqual(coverageDecisionsFor({ ...springs, published_entity_id: undefined }, publicSnapshot), []);
const commerce = municipalities.find((record) => record.id === "us-co-place-0816495");
const v2Snapshot = {
  mode: "public",
  entities: [
    { id: "jurisdiction_commerce", type: "JURISDICTION", jurisdiction_ref: commerce.id },
    { id: "decision_commerce", type: "DECISION", slug: "commerce", jurisdiction_ids: ["jurisdiction_commerce"] },
  ],
};
assert.deepEqual(coverageDecisionsFor(commerce, v2Snapshot).map((decision) => decision.id), ["decision_commerce"]);
const indexed = coverageDecisionIndex(registry.jurisdictions, v2Snapshot);
assert.deepEqual(indexed.get(commerce.id).map((decision) => decision.id), ["decision_commerce"]);
assert.equal(indexed.has(springs.id), false);
assert.equal(coverageDecisionIndex(registry.jurisdictions, { ...v2Snapshot, mode: "preview" }).size, 0);

console.log("Colorado geography registry and case-link boundary: 4,111 records verified");
