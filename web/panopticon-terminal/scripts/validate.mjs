import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const templates = ["templates/shell-00.html", "templates/shell-01.html", "templates/shell-02.html", "templates/shell-03.html"];
const scripts = [
  "scripts/00-core.js", "scripts/12-helpers.js", "scripts/01-boot.js",
  "scripts/02-status-activity.js", "scripts/03-dossier.js", "scripts/04-evidence.js",
  "scripts/05-view-search.js", "scripts/06-commands.js", "scripts/07-timeline.js",
  "scripts/08-globe-base.js", "scripts/09-globe-world.js", "scripts/10-globe-data.js",
  "scripts/11-globe-input.js",
];
const styles = Array.from({ length: 8 }, (_, index) => `styles/0${index}.css`);
const required = ["index.html", "styles.css", "app.js", "api.js", "mock/public-snapshot.json", ...templates, ...scripts, ...styles];
const failures = [];
const contents = new Map();

for (const path of required) {
  try {
    contents.set(path, await readFile(join(root, path), "utf8"));
  } catch (error) {
    failures.push(`${path}: cannot read (${error.message})`);
  }
}

const index = contents.get("index.html") ?? "";
for (const asset of ["./styles.css", "./app.js"]) {
  if (!index.includes(asset)) failures.push(`index.html: missing asset reference ${asset}`);
}

const shell = templates.map((path) => contents.get(path) ?? "").join("");
for (const id of [
  "app", "world-canvas", "main-content", "global-search", "activity-list",
  "entity-workspace", "evidence-drawer", "search-dialog", "search-input",
  "search-results", "command-dialog", "timeline-input",
]) {
  if (!shell.includes(`id="${id}"`)) failures.push(`terminal shell: missing required #${id}`);
}

const ids = [...shell.matchAll(/\bid="([^"]+)"/g)].map((match) => match[1]);
const duplicateIds = ids.filter((id, index) => ids.indexOf(id) !== index);
for (const id of [...new Set(duplicateIds)]) failures.push(`terminal shell: duplicate #${id}`);

const app = contents.get("app.js") ?? "";
for (const path of [...templates, ...scripts]) {
  if (!app.includes(`./${path}`)) failures.push(`app.js: missing module asset ./${path}`);
}
if (!app.includes("new PanopticonClient")) failures.push("app.js: missing PanopticonClient bootstrap");

const rootStyles = contents.get("styles.css") ?? "";
for (const path of styles) {
  if (!rootStyles.includes(`./${path}`)) failures.push(`styles.css: missing import ./${path}`);
}

let snapshot;
try {
  snapshot = JSON.parse(contents.get("mock/public-snapshot.json") ?? "");
} catch (error) {
  failures.push(`mock/public-snapshot.json: invalid JSON (${error.message})`);
}

if (snapshot) {
  if (snapshot.demo !== true) failures.push("snapshot: demo must be true");
  if (snapshot.schema_version !== "pnull-public-snapshot-v1") failures.push("snapshot: unexpected schema_version");
  if (!snapshot.status?.manifest_digest?.startsWith("sha256:")) failures.push("snapshot: status.manifest_digest must be sha256-prefixed");

  const entities = new Map((snapshot.entities ?? []).map((item) => [item.id, item]));
  const sources = new Map((snapshot.sources ?? []).map((item) => [item.id, item]));
  const evidence = new Map((snapshot.evidence ?? []).map((item) => [item.id, item]));
  if (!entities.size || !sources.size || !evidence.size) failures.push("snapshot: entities, sources, and evidence must be non-empty");

  for (const [id, entity] of entities) {
    if (!id) failures.push("snapshot: entity with empty id");
    if (!(entity.tags ?? []).includes("DEMO DATA")) failures.push(`entity ${id}: missing DEMO DATA tag`);
    for (const sourceId of entity.source_ids ?? []) if (!sources.has(sourceId)) failures.push(`entity ${id}: missing source ${sourceId}`);
    for (const relationship of entity.relationships ?? []) {
      if (!entities.has(relationship.target_entity_id)) failures.push(`entity ${id}: missing relationship target ${relationship.target_entity_id}`);
      if (relationship.evidence_id && !evidence.has(relationship.evidence_id)) failures.push(`entity ${id}: missing relationship evidence ${relationship.evidence_id}`);
    }
    for (const attribute of entity.attributes ?? []) if (attribute.evidence_id && !evidence.has(attribute.evidence_id)) failures.push(`entity ${id}: missing attribute evidence ${attribute.evidence_id}`);
    for (const event of entity.timeline ?? []) if (event.evidence_id && !evidence.has(event.evidence_id)) failures.push(`entity ${id}: missing event evidence ${event.evidence_id}`);
  }

  for (const [id, source] of sources) {
    if (source.demo !== true) failures.push(`source ${id}: demo must be true`);
    if (!source.sha256?.match(/^[a-f0-9]{64}$/)) failures.push(`source ${id}: invalid sha256`);
    try {
      if (new URL(source.canonical_url).protocol !== "https:") failures.push(`source ${id}: canonical_url must use https`);
    } catch {
      failures.push(`source ${id}: invalid canonical_url`);
    }
  }

  for (const [id, record] of evidence) {
    if (record.demo !== true) failures.push(`evidence ${id}: demo must be true`);
    if (!sources.has(record.source_id)) failures.push(`evidence ${id}: missing source ${record.source_id}`);
    if (record.review_state !== "APPROVED") failures.push(`evidence ${id}: public demo evidence must be APPROVED`);
    if (!record.review_bound_digest?.startsWith("sha256:")) failures.push(`evidence ${id}: missing review binding digest`);
    if (!record.sha256?.match(/^[a-f0-9]{64}$/)) failures.push(`evidence ${id}: invalid source sha256`);
  }

  for (const item of snapshot.activity ?? []) {
    if (item.entity_id && !entities.has(item.entity_id)) failures.push(`activity ${item.id}: missing entity ${item.entity_id}`);
  }
}

if (failures.length) {
  console.error("Public terminal validation failed:\n");
  failures.forEach((failure) => console.error(`- ${failure}`));
  process.exitCode = 1;
} else {
  console.log("Public terminal validation passed.");
}
