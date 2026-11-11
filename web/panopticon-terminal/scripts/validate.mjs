import { lstat, readFile, readdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";
import { createHash } from "node:crypto";
import { validateSnapshot } from "../api.js";
import { votePortraits } from "../portraits/portraits.js";
import { matchesDenverGallery } from "../portraits/lookup.js";
import { validateResearch } from "../research-api.js";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const repositoryRoot = resolve(process.env.PNUL_PORTRAIT_PROOF_ROOT ?? join(root, "..", ".."));
const templates = ["templates/shell-00.html", "templates/shell-01.html", "templates/shell-02.html", "templates/shell-03.html"];
const scripts = [
  "scripts/00-core.js", "scripts/12-helpers.js", "scripts/01-boot.js",
  "scripts/02-status-activity.js", "scripts/03-dossier.js", "scripts/04-evidence.js",
  "scripts/05-view-search.js", "scripts/06-commands.js", "scripts/07-timeline.js",
  "scripts/08-globe-base.js", "scripts/09-globe-world.js", "scripts/10-globe-data.js",
  "scripts/11-globe-input.js", "scripts/13-civic-data.js", "scripts/14-civic-shell.js",
  "scripts/15-civic-actions.js", "scripts/16-civic-context.js", "scripts/17-civic-atlas-core.js",
  "scripts/18-civic-atlas-layout.js", "scripts/19-civic-atlas-ground.js", "scripts/20-civic-atlas-links.js",
  "scripts/21-civic-atlas-helpers.js", "scripts/22-civic-runtime.js", "scripts/23-civic-polish.js",
  "scripts/24-colorado-coverage.mjs",
];
const styles = [
  ...Array.from({ length: 8 }, (_, index) => `styles/0${index}.css`),
  "styles/08-civic.css",
  "styles/09-civic-polish.css",
  "styles/10-colorado-coverage.css",
];
const required = ["index.html", "styles.css", "app.js", "api.js", "decision.html", "decision.css", "decision.js", "research.html", "research.css", "research.js", "research-api.js", "public/research-signals.json", "portraits/portraits.js", "portraits/lookup.js", "mock/public-snapshot.json", "public/public-snapshot.json", "geography/colorado-jurisdictions.json", "scripts/build-decision-pages.mjs", "scripts/filter-portraits.mjs", ...templates, ...scripts, ...styles];
const failures = [];
const contents = new Map();

for (const path of required) {
  try {
    if (path === "public/research-signals.json" && (await lstat(join(root, path))).size > 8 * 1024 * 1024) throw new Error("research report exceeds byte limit");
    contents.set(path, await readFile(join(root, path), "utf8"));
  } catch (error) {
    failures.push(`${path}: cannot read (${error.message})`);
  }
}

try {
  const research = validateResearch(JSON.parse(contents.get("public/research-signals.json") ?? ""));
  const people = research.records.flatMap((item) => [
    ...(item.people_context ?? []),
    ...(item.actions ?? []).flatMap((motion) => motion.votes),
  ]);
  const portraits = new Map(people.filter((person) => person.portrait).map((person) => [person.portrait.src, person.portrait]));
  for (const [path, portrait] of portraits) {
    const meta = await lstat(join(root, path));
    if (!meta.isFile() || meta.isSymbolicLink() || meta.size > 2_000_000) throw new Error("research portrait is not a bounded regular file");
    const bytes = await readFile(join(root, path));
    if (createHash("sha256").update(bytes).digest("hex") !== portrait.sha256) throw new Error("research portrait digest mismatch");
    const jpeg = bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff;
    const png = bytes.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]));
    const webp = bytes.toString("ascii", 0, 4) === "RIFF" && bytes.toString("ascii", 8, 12) === "WEBP";
    if (!((path.endsWith(".jpg") && jpeg) || (path.endsWith(".png") && png) || (path.endsWith(".webp") && webp))) throw new Error("research portrait file type mismatch");
  }
}
catch (error) { failures.push(`preliminary research: ${error.message}`); }

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
if (!app.includes("The Public Record, Connected")) failures.push("app.js: missing civic product identity");
if (!app.includes('params.get("demo") === "1"') || !app.includes('params.get("preview") === "1"')) failures.push("app.js: demo and preview must be explicit opt-ins");

const decisionHtml = contents.get("decision.html") ?? "";
for (const asset of ["./decision.css", "./decision.js"]) {
  if (!decisionHtml.includes(asset)) failures.push(`decision.html: missing asset reference ${asset}`);
}
if (!(contents.get("scripts/03-dossier.js") ?? "").includes("decision.html?decision=")) failures.push("dossier: missing shareable decision link");

const rootStyles = contents.get("styles.css") ?? "";
for (const path of styles) {
  if (!rootStyles.includes(`./${path}`)) failures.push(`styles.css: missing import ./${path}`);
}

const civicShell = contents.get("scripts/14-civic-shell.js") ?? "";
for (const phrase of ["THE PUBLIC RECORD, CONNECTED", "What is happening here?", "PUBLIC INTELLIGENCE COMMONS"]) {
  if (!civicShell.includes(phrase)) failures.push(`civic shell: missing product language: ${phrase}`);
}

const civicActions = contents.get("scripts/15-civic-actions.js") ?? "";
for (const phrase of ["RECORDED VOTERS", "names in public vote record", "No device-location access"]) {
  if (!civicActions.includes(phrase) && !civicShell.includes(phrase)) failures.push(`civic boundary: missing language: ${phrase}`);
}

const civicPolish = contents.get("scripts/23-civic-polish.js") ?? "";
for (const phrase of ["POWER LEAVES A RECORD. WE CONNECT IT.", "No private-person tracking"]) {
  if (!civicPolish.includes(phrase)) failures.push(`civic polish: missing boundary or identity: ${phrase}`);
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
    if (record.review_state !== "DEMO") failures.push(`evidence ${id}: synthetic evidence must be DEMO`);
    if (!record.verification_digest?.startsWith("sha256:")) failures.push(`evidence ${id}: missing verification digest`);
    if (!record.sha256?.match(/^[a-f0-9]{64}$/)) failures.push(`evidence ${id}: invalid source sha256`);
  }

  for (const item of snapshot.activity ?? []) {
    if (item.entity_id && !entities.has(item.entity_id)) failures.push(`activity ${item.id}: missing entity ${item.entity_id}`);
  }
}

try {
  const published = validateSnapshot(JSON.parse(contents.get("public/public-snapshot.json") ?? ""));
  const decisions = published.entities.filter((entity) => entity.type === "DECISION");
  const decision = decisions.find((entity) => entity.slug === "co-colorado-springs-25-581-2025-11-25");
  const voters = new Map(published.entities.filter((entity) => entity.type === "RECORDED_VOTER").map((entity) => [entity.id, entity]));
  if (process.env.PNUL_VALIDATE_RELEASE === "1") {
    const releasedVoterIds = new Set(decisions.flatMap((item) => [
      ...(item.votes ?? []), ...(item.consent_block?.roll_call ?? []),
    ]).map((vote) => vote.voter_entity_id));
    for (const id of Object.keys(votePortraits)) {
      if (!releasedVoterIds.has(id)) failures.push(`portrait: unreleased voter in public registry ${id}`);
    }
  }
  for (const vote of decision?.votes ?? []) {
    if (!votePortraits[vote.voter_entity_id]) failures.push(`portrait: no image for ${vote.name} (${vote.voter_entity_id})`);
  }
  const registry = JSON.parse(await readFile(join(repositoryRoot, "fixtures/atlas/sources.co.v1.json"), "utf8"));
  const sourceById = new Map(registry.sources.map((source) => [source.id, source]));
  for (const folder of ["cases", "preview-cases"]) {
    const directory = join(repositoryRoot, "fixtures/atlas", folder);
    for (const file of await readdir(directory)) {
      if (!file.endsWith(".v2.json")) continue;
      const manifest = JSON.parse(await readFile(join(directory, file), "utf8"));
      for (const vote of manifest.votes ?? []) {
        const source = sourceById.get(vote.person_name.proof.source_id);
        const client = source && new URL(source.url).pathname.split("/")[2];
        const personId = vote.official_person_id.value;
        if (!client || !/^[a-z0-9-]+$/.test(client) || !/^\d+$/.test(personId)) {
          failures.push(`portrait: invalid official voter identity in ${file}`);
          continue;
        }
        const id = `recorded_voter_${manifest.jurisdiction.id.replaceAll("-", "_")}_${client.replaceAll("-", "_")}_${personId}`;
        const expected = { id, type: "RECORDED_VOTER", name: vote.person_name.value,
          jurisdiction_ref: manifest.jurisdiction.jurisdiction_ref, identity_scope: `legistar:${client}`,
          official_person_id: personId };
        const previous = voters.get(id);
        if (previous && ["name", "jurisdiction_ref", "identity_scope", "official_person_id"].some((key) => previous[key] !== expected[key])) {
          failures.push(`portrait: conflicting official voter identity ${id}`);
        } else voters.set(id, expected);
      }
    }
  }
  for (const [id, portrait] of Object.entries(votePortraits)) {
    const voter = voters.get(id);
    if (!voter || voter.name !== portrait.name) failures.push(`portrait: unrecognized or mismatched voter ${id}`);
    for (const field of ["jurisdiction_ref", "identity_scope", "official_person_id"]) {
      if (voter?.[field] !== undefined && String(voter[field]) !== String(portrait[field])) failures.push(`portrait: ${field} mismatch for ${id}`);
    }
    if (!/^\.\/portraits\/[a-z0-9-]+\.(jpe?g|png|webp)$/.test(portrait.src)) failures.push(`portrait: unsafe image path for ${id}`);
    try {
      const pageUrl = new URL(portrait.source_url);
      const imageUrl = new URL(portrait.image_url);
      if (pageUrl.protocol !== "https:" || imageUrl.protocol !== "https:" ||
          pageUrl.hostname.replace(/^www\./, "") !== imageUrl.hostname.replace(/^www\./, "") ||
          pageUrl.username || pageUrl.password ||
          imageUrl.username || imageUrl.password || !portrait.credit) {
        failures.push(`portrait: unsafe official source for ${id}`);
      }
    } catch { failures.push(`portrait: invalid official URL for ${id}`); }
    if (!/^[a-f0-9]{64}$/.test(portrait.sha256)) failures.push(`portrait: invalid digest for ${id}`);
    if (/^\.\/portraits\/[a-z0-9-]+\.(jpe?g|png|webp)$/.test(portrait.src)) {
      try {
        const path = join(root, portrait.src);
        const meta = await lstat(path);
        if (!meta.isFile() || meta.isSymbolicLink() || meta.size > 5_000_000) throw new Error("not a bounded regular image");
        const bytes = await readFile(path);
        const isPng = bytes.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]));
        const isJpeg = bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff;
        const isWebp = bytes.toString("ascii", 0, 4) === "RIFF" && bytes.toString("ascii", 8, 12) === "WEBP";
        const extension = portrait.src.split(".").at(-1).toLowerCase();
        if (!((extension === "png" && isPng) || (["jpg", "jpeg"].includes(extension) && isJpeg) ||
            (extension === "webp" && isWebp))) failures.push(`portrait: image bytes do not match file type for ${id}`);
        const digest = createHash("sha256").update(bytes).digest("hex");
        if (digest !== portrait.sha256) failures.push(`portrait: digest mismatch for ${id}`);
      } catch (error) { failures.push(`portrait: cannot read ${portrait.src} (${error.message})`); }
    }
    if (voter?.identity_scope) {
      const captureKind = portrait.source_capture_kind ?? "raw_html";
      const expectedExtension = captureKind === "browser_dom_extract" ? "json" : "html";
      if (!["browser_dom_extract", "raw_html", "official_gallery"].includes(captureKind) ||
          !new RegExp(`^fixtures/atlas/portrait-sources/[a-z0-9-]+\\.${expectedExtension}$`).test(portrait.source_page_fixture ?? "") ||
          !/^[a-f0-9]{64}$/.test(portrait.source_page_sha256 ?? "") ||
          portrait.verification_state !== "SOURCE_BYTES_VERIFIED" ||
          portrait.portrait_date !== "not_established" ||
          !/^\d{4}-\d{2}-\d{2}$/.test(portrait.retrieved_on ?? "")) {
        failures.push(`portrait: incomplete v2 provenance for ${id}`);
        continue;
      }
      try {
        const pagePath = join(repositoryRoot, portrait.source_page_fixture);
        const meta = await lstat(pagePath);
        if (!meta.isFile() || meta.isSymbolicLink() || meta.size > 2_000_000) throw new Error("not a bounded regular source page");
        const bytes = await readFile(pagePath);
        if (createHash("sha256").update(bytes).digest("hex") !== portrait.source_page_sha256) failures.push(`portrait: source page digest mismatch for ${id}`);
        let profileEmails = [];
        if (captureKind === "official_gallery") {
          if (!matchesDenverGallery(portrait, voter, bytes.toString("utf8"))) {
            failures.push(`portrait: official gallery does not bind voter and image for ${id}`);
          }
        } else if (captureKind === "browser_dom_extract") {
          const capture = JSON.parse(bytes.toString("utf8"));
          const headingSelector = "h2.directory-page-title .fw-bold";
          const imageSelector = '.directory-staff-container img[src*="ImageRepository/Document"]';
          const emailSelector = '.directory-staff-container a[href^="mailto:"]';
          const observedImage = new URL(capture.image_src ?? "", portrait.source_url);
          if (observedImage.protocol === "http:") observedImage.protocol = "https:";
          const locator = `${headingSelector}; ${imageSelector}; ${emailSelector}`;
          if (capture.capture_kind !== captureKind || capture.source_url !== portrait.source_url ||
              capture.retrieved_on !== portrait.retrieved_on ||
              capture.heading_selector !== headingSelector || capture.image_selector !== imageSelector ||
              capture.email_selector !== emailSelector || capture.heading_text !== portrait.name ||
              capture.image_alt !== portrait.source_image_alt ||
              observedImage.href !== portrait.image_url || portrait.display_image_url !== portrait.image_url ||
              portrait.source_page_locator !== locator) {
            failures.push(`portrait: browser capture does not bind name and image for ${id}`);
          }
          if (typeof capture.email_href === "string" && capture.email_href.toLowerCase().startsWith("mailto:")) {
            profileEmails = [capture.email_href.slice(7).toLowerCase()];
          }
        } else {
          const html = bytes.toString("utf8");
          const heading = /<h1\b[^>]*class=['"][^'"]*\boc-page-title\b[^'"]*['"][^>]*>([^<]+)<\/h1>/i.exec(html)?.[1]?.trim();
          const pageName = portrait.source_page_name ?? portrait.name;
          const imageAlt = portrait.source_image_alt ?? `${portrait.name} headshot`;
          const imageTag = [...html.matchAll(/<img\b[^>]*>/gi)].map((match) => match[0])
            .find((tag) => /\bclass="[^"]*\bmain-page-image\b[^"]*"/i.test(tag) &&
              /\balt="([^"]+)"/i.exec(tag)?.[1] === imageAlt);
          const imagePath = imageTag && /\bsrc="([^"]+)"/i.exec(imageTag)?.[1];
          const locator = `h1.oc-page-title; img${portrait.source_image_alt ? ".main-page-image" : ""}[alt="${imageAlt}"]` +
            (portrait.identity_bridge === "official_email" ? '; li.contact-email a[href^="mailto:"]' : "");
          if (heading !== pageName || !imagePath ||
              new URL(imagePath, portrait.source_url).href !== portrait.display_image_url ||
              portrait.source_page_locator !== locator ||
              (pageName !== portrait.name && portrait.identity_bridge !== "official_email") ||
              (portrait.identity_bridge === "exact_name" && pageName !== portrait.name)) {
            failures.push(`portrait: page does not bind name and image for ${id}`);
          }
          profileEmails = [...html.matchAll(/<li\b[^>]*class="[^"]*\bcontact-email\b[^"]*"[^>]*>([\s\S]*?)<\/li>/gi)]
            .flatMap((item) => [...item[1].matchAll(/<a\b[^>]*href="mailto:([^"]+)"/gi)]
              .map((link) => link[1].toLowerCase()));
        }
        if (portrait.identity_bridge && !["exact_name", "official_email"].includes(portrait.identity_bridge)) {
          failures.push(`portrait: unsupported identity bridge for ${id}`);
        }
        if (portrait.identity_bridge === "official_email" && !portrait.person_record_fixture) {
          failures.push(`portrait: alias requires official person record for ${id}`);
        }
        if (portrait.person_record_fixture) {
          const client = voter.identity_scope.slice("legistar:".length);
          const expectedPersonUrl = `https://webapi.legistar.com/v1/${client}/Persons/${voter.official_person_id}`;
          if (portrait.person_record_url !== expectedPersonUrl ||
              portrait.person_record_fixture !== `fixtures/atlas/portrait-sources/${client}-person-${voter.official_person_id}.json` ||
              !/^[a-f0-9]{64}$/.test(portrait.person_record_sha256 ?? "")) {
            failures.push(`portrait: invalid official person record reference for ${id}`);
          } else {
            const recordPath = join(repositoryRoot, portrait.person_record_fixture);
            const recordMeta = await lstat(recordPath);
            if (!recordMeta.isFile() || recordMeta.isSymbolicLink() || recordMeta.size > 100_000) throw new Error("not a bounded regular person record");
            const personBytes = await readFile(recordPath);
            if (createHash("sha256").update(personBytes).digest("hex") !== portrait.person_record_sha256) {
              failures.push(`portrait: official person record digest mismatch for ${id}`);
            }
            const person = JSON.parse(personBytes.toString("utf8"));
            const timConflict = id === "recorded_voter_co_castle_rock_castlerock_co_494";
            const expectedLocator = `$.PersonId; $.PersonFullName${portrait.identity_bridge === "official_email" ? "; $.PersonEmail" : ""}${timConflict ? "; $.PersonWWW" : ""}`;
            if (String(person.PersonId) !== voter.official_person_id || person.PersonFullName !== voter.name ||
                portrait.person_record_locator !== expectedLocator) {
              failures.push(`portrait: official person record does not bind voter for ${id}`);
            }
            if (timConflict) {
              const conflict = portrait.source_conflict;
              const maxProfile = votePortraits.recorded_voter_co_castle_rock_castlerock_co_342?.source_url;
              if (conflict?.state !== "UNRESOLVED" || conflict.field !== "PersonWWW" ||
                  conflict.recorded_url !== "http://crgov.com/directory.aspx?eid=20" ||
                  conflict.recorded_url !== person.PersonWWW || conflict.profile_url !== portrait.source_url ||
                  new URL(maxProfile).searchParams.get("eid") !== "20" ||
                  conflict.public_note !== "Source conflict: the portal's website field points to another council profile. The portrait match uses the town's name and email records.") {
                failures.push(`portrait: unresolved portal profile conflict is not disclosed for ${id}`);
              }
            } else if (portrait.source_conflict) {
              failures.push(`portrait: unreviewed source conflict field for ${id}`);
            }
            if (portrait.identity_bridge === "official_email") {
              const email = String(person.PersonEmail ?? "").toLowerCase();
              const officialEmailHost = new URL(portrait.source_url).hostname.replace(/^www\./, "");
              if (!/^[a-z0-9._%+-]+@[a-z0-9.-]+$/.test(email) ||
                  !email.endsWith(`@${officialEmailHost}`) ||
                  !profileEmails.includes(email)) {
                failures.push(`portrait: official email bridge does not match profile for ${id}`);
              }
            }
          }
        }
        const displayed = new URL(portrait.display_image_url);
        const original = new URL(portrait.image_url);
        if (captureKind === "raw_html" &&
            (displayed.origin !== original.origin || displayed.pathname !== original.pathname || original.search || original.hash)) {
          failures.push(`portrait: original image URL is not the page's image path for ${id}`);
        }
      } catch (error) { failures.push(`portrait: cannot verify source page for ${id} (${error.message})`); }
    }
  }
  const genericSocialUrl = decisionHtml.match(/property="og:url" content="([^"]+)"/)?.[1];
  if (!genericSocialUrl?.endsWith("/decision.html") || decisions.some((item) => item.share_url === genericSocialUrl) ||
      !decisionHtml.includes('name="robots" content="noindex,follow"') ||
      decisionHtml.includes("Colorado Springs police technology surcharge vote")) {
    failures.push("decision.html: generic route claims a specific decision");
  }
  if (!decisionHtml.includes('name="twitter:card" content="summary"')) failures.push("decision.html: missing X summary card metadata");
  if (process.env.PNUL_VALIDATE_RELEASE === "1") {
    for (const item of decisions) {
      const slug = item.slug;
      if (!/^co-[a-z0-9-]+$/.test(slug ?? "")) { failures.push(`decision ${item.id}: unsafe slug`); continue; }
      if (!item.share_url?.endsWith(`/decisions/${slug}.html`)) continue;
      const path = `decisions/${slug}.html`;
      try {
        const html = await readFile(join(root, path), "utf8");
        if (!html.includes(`property="og:url" content="${item.share_url}"`)) failures.push(`${path}: wrong social URL`);
        if (!html.includes(`rel="canonical" href="${item.share_url}"`)) failures.push(`${path}: wrong canonical URL`);
        if (!html.includes('name="robots" content="index,follow"') || !html.includes('property="og:type" content="article"')) failures.push(`${path}: case metadata is not indexable article content`);
        if (!html.includes('href="../decision.css"') || !html.includes('src="../decision.js"')) failures.push(`${path}: broken relative assets`);
      } catch (error) { failures.push(`${path}: missing static case page (${error.message})`); }
    }
  }
} catch (error) {
  failures.push(`public/public-snapshot.json: ${error.message}`);
}

if (failures.length) {
  console.error("Public terminal validation failed:\n");
  failures.forEach((failure) => console.error(`- ${failure}`));
  process.exitCode = 1;
} else {
  console.log("Public terminal validation passed.");
}
