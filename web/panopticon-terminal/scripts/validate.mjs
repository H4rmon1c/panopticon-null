import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const requiredFiles = ["index.html", "styles.css", "app.js", "api.js", "mock/public-snapshot.json"];
const failures = [];

const contents = new Map();
for (const path of requiredFiles) {
  try {
    contents.set(path, await readFile(join(root, path), "utf8"));
  } catch (error) {
    failures.push(`${path}: cannot read (${error.message})`);
  }
}

const html = contents.get("index.html") ?? "";
for (const id of [
  "app",
  "main-content",
  "global-search",
  "search-results",
  "activity-list",
  "entity-workspace",
  "evidence-drawer",
  "command-dialog",
]) {
  if (!html.includes(`id="${id}"`)) failures.push(`index.html: missing required #${id}`);
}

for (const asset of ["./styles.css", "./app.js"]) {
  if (!html.includes(asset)) failures.push(`index.html: missing asset reference ${asset}`);
}

let snapshot;
try {
  snapshot = JSON.parse(contents.get("mock/public-snapshot.json") ?? "");
} catch (error) {
  failures.push(`mock/public-snapshot.json: invalid JSON (${error.message})`);
}

if (snapshot) {
  if (snapshot.demo !== true) failures.push("snapshot: demo must be true");
  if (snapshot.schema_version !== "pnull-public-snapshot-v1") {
    failures.push("snapshot: unexpected schema_version");
  }
  if (!snapshot.status?.manifest_digest?.startsWith("sha256:")) {
    failures.push("snapshot: status.manifest_digest must be a sha256 value");
  }

  const entities = new Map((snapshot.entities ?? []).map((item) => [item.id, item]));
  const sources = new Map((snapshot.sources ?? []).map((item) => [item.id, item]));
  const evidence = new Map((snapshot.evidence ?? []).map((item) => [item.id, item]));

  for (const [kind, records] of [["entity", entities], ["source", sources], ["evidence", evidence]]) {
    if (!records.size) failures.push(`snapshot: no ${kind} records`);
    for (const [id, record] of records) {
      if (!id) failures.push(`snapshot: ${kind} with empty id`);
      if (record.demo !== true && kind !== "entity") failures.push(`${kind} ${id}: demo must be true`);
    }
  }

  for (const [id, entity] of entities) {
    if (!(entity.tags ?? []).includes("DEMO DATA")) failures.push(`entity ${id}: missing DEMO DATA tag`);
    for (const sourceId of entity.source_ids ?? []) {
      if (!sources.has(sourceId)) failures.push(`entity ${id}: missing source ${sourceId}`);
    }
    for (const relationship of entity.relationships ?? []) {
      if (!entities.has(relationship.target_entity_id)) {
        failures.push(`entity ${id}: relationship target missing: ${relationship.target_entity_id}`);
      }
      if (relationship.evidence_id && !evidence.has(relationship.evidence_id)) {
        failures.push(`entity ${id}: relationship evidence missing: ${relationship.evidence_id}`);
      }
    }
    for (const attribute of entity.attributes ?? []) {
      if (attribute.evidence_id && !evidence.has(attribute.evidence_id)) {
        failures.push(`entity ${id}: attribute evidence missing: ${attribute.evidence_id}`);
      }
    }
    for (const event of entity.timeline ?? []) {
      if (event.evidence_id && !evidence.has(event.evidence_id)) {
        failures.push(`entity ${id}: event evidence missing: ${event.evidence_id}`);
      }
    }
  }

  for (const [id, source] of sources) {
    if (source.demo !== true) failures.push(`source ${id}: demo must be true`);
    if (!source.sha256?.match(/^[a-f0-9]{64}$/)) failures.push(`source ${id}: invalid sha256`);
    try {
      const url = new URL(source.canonical_url);
      if (url.protocol !== "https:") failures.push(`source ${id}: canonical_url must use https`);
    } catch {
      failures.push(`source ${id}: invalid canonical_url`);
    }
  }

  for (const [id, record] of evidence) {
    if (!sources.has(record.source_id)) failures.push(`evidence ${id}: missing source ${record.source_id}`);
    if (record.review_state !== "APPROVED") failures.push(`evidence ${id}: demo public evidence must be APPROVED`);
    if (!record.review_bound_digest?.startsWith("sha256:")) failures.push(`evidence ${id}: missing review binding digest`);
    if (!record.sha256?.match(/^[a-f0-9]{64}$/)) failures.push(`evidence ${id}: invalid source sha256`);
  }

  for (const item of snapshot.activity ?? []) {
    if (item.entity_id && !entities.has(item.entity_id)) failures.push(`activity ${item.id}: missing entity ${item.entity_id}`);
  }
}

if (failures.length) {
  console.error("Public terminal validation failed:\n");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exitCode = 1;
} else {
  console.log("Public terminal validation passed.");
}
