import { validateGeorgetownMotion, validateGeorgetownResearchBinding } from "../georgetown-motion-api.js";
import { validateHudsonMotion, validateHudsonResearchBinding } from "../hudson-motion-api.js";
import { validateBuenaVistaMotion, validateBuenaVistaResearchBinding } from "../buenavista-motion-api.js";
import { lstat, readFile, readdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";
import { createHash } from "node:crypto";
import { validateSnapshot } from "../api.js";
import { votePortraits } from "../portraits/portraits.js";
import { matchesDenverGallery } from "../portraits/lookup.js";
import { validateResearch } from "../research-api.js";
import { validateClearCreekMotion, validateClearCreekResearchBinding } from "../clearcreek-motion-api.js";
import { validateColumbineMotion, validateColumbineResearchBinding } from "../columbine-motion-api.js";
import { validateGunnisonMotion, validateGunnisonResearchBinding } from "../gunnison-motion-api.js";
import { validateOurayMotion, validateOurayResearchBinding } from "../ouray-motion-api.js";
import { yumaPortraits, yumaPortraitFor } from "../yuma-portraits-api.js";
import { validateYumaMotion, validateYumaResearchBinding } from "../yuma-motion-api.js";
import { validateCortezMotion, validateCortezResearchBinding } from "../cortez-motion-api.js";
import { cortezPortraits, cortezPortraitFor } from "../cortez-portrait-api.js";
import { validateKeenesburgMotion, validateKeenesburgResearchBinding } from "../keenesburg-motion-api.js";
import { validateGardenMotion, validateGardenResearchBinding } from "../garden-motion-api.js";
import { gardenPortraits, validateGardenPortraitProof } from "../garden-portraits-api.js";
import { assertEmbeddedPortraitMatches } from "./validate-embedded-portrait.mjs";

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
const required = ["georgetown-motion.html", "georgetown-motion.js", "georgetown-motion-api.js", "public/georgetown-motion-v1.json", "hudson-motion.html", "hudson-motion.js", "hudson-motion-api.js", "public/hudson-motion-v1.json", "index.html", "styles.css", "app.js", "api.js", "decision.html", "decision.css", "decision.js", "research.html", "research.css", "research.js", "research-api.js", "buenavista-motion.html", "buenavista-motion.js", "buenavista-motion-api.js", "public/buenavista-motion-v1.json", "columbine-motion.html", "columbine-motion.js", "columbine-motion-api.js", "public/columbine-motion-v1.json", "gunnison-motion.html", "gunnison-motion.js", "gunnison-motion-api.js", "public/gunnison-motion-v1.json", "clearcreek-motion.html", "clearcreek-motion.js", "clearcreek-motion-api.js", "public/clearcreek-motion-v1.json", "ouray-motion.html", "ouray-motion.js", "ouray-motion-api.js", "public/ouray-motion-v1.json", "yuma-motion.html", "yuma-motion.js", "yuma-motion-api.js", "public/yuma-motion-v1.json", "cortez-motion.html", "cortez-motion.js", "cortez-motion-api.js", "public/cortez-motion-v1.json", "keenesburg-motion.html", "keenesburg-motion.js", "keenesburg-motion-api.js", "public/keenesburg-motion-v1.json", "garden-motion.html", "garden-motion.js", "garden-motion-api.js", "garden-portraits-api.js", "public/garden-city-motion-v1.json", "public/research-signals.json", "portraits/portraits.js", "portraits/lookup.js", "mock/public-snapshot.json", "public/public-snapshot.json", "geography/colorado-jurisdictions.json", "scripts/build-decision-pages.mjs", "scripts/filter-portraits.mjs", ...templates, ...scripts, ...styles];
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
  const clearcreek = validateClearCreekMotion(JSON.parse(contents.get("public/clearcreek-motion-v1.json") ?? ""));
  validateClearCreekResearchBinding(clearcreek, research);
  const georgetown = validateGeorgetownMotion(JSON.parse(contents.get("public/georgetown-motion-v1.json") ?? ""));
  validateGeorgetownResearchBinding(georgetown, research);
  const hudson = validateHudsonMotion(JSON.parse(contents.get("public/hudson-motion-v1.json") ?? ""));
  validateHudsonResearchBinding(hudson, research);
  const buenavista = validateBuenaVistaMotion(JSON.parse(contents.get("public/buenavista-motion-v1.json") ?? ""));
  validateBuenaVistaResearchBinding(buenavista, research);
  const columbine = validateColumbineMotion(JSON.parse(contents.get("public/columbine-motion-v1.json") ?? ""));
  validateColumbineResearchBinding(columbine, research);
  const gunnison = validateGunnisonMotion(JSON.parse(contents.get("public/gunnison-motion-v1.json") ?? ""));
  validateGunnisonResearchBinding(gunnison, research);
  const ouray = validateOurayMotion(JSON.parse(contents.get("public/ouray-motion-v1.json") ?? ""));
  validateOurayResearchBinding(ouray, research);
  const yuma = validateYumaMotion(JSON.parse(contents.get("public/yuma-motion-v1.json") ?? ""));
  validateYumaResearchBinding(yuma, research);
  if (yuma.state === 'available' && yuma.attendance.filter(person=>yumaPortraitFor(person)).length !== yumaPortraits.length)
    throw new Error('Yuma portrait attendance binding differs');
  for (const portrait of yumaPortraits) {
    const path=join(root,portrait.src);let meta;
    try {meta=await lstat(path);} catch (error) {if(error.code!=='ENOENT')throw error;}
    if(meta){if(!meta.isFile()||meta.isSymbolicLink()||meta.size!==portrait.bytes)throw new Error('Yuma portrait file differs');
      const body=await readFile(path);
      if(body[0]!==0xff||body[1]!==0xd8||body[2]!==0xff||createHash('sha256').update(body).digest('hex')!==portrait.sha256)
        throw new Error('Yuma portrait digest or type differs');}
  }
  const cortez = validateCortezMotion(JSON.parse(contents.get("public/cortez-motion-v1.json") ?? ""));
  validateCortezResearchBinding(cortez, research);
  if (cortez.state === 'available' && cortez.votes.filter(vote => cortezPortraitFor(vote)).length !== cortezPortraits.length)
    throw new Error('Cortez portrait voter binding differs');
  for (const portrait of cortezPortraits) {
    const path = join(root, portrait.src); let meta;
    try { meta = await lstat(path); } catch (error) { if (error.code !== 'ENOENT') throw error; }
    if (meta) {
      if (!meta.isFile() || meta.isSymbolicLink() || meta.size !== portrait.bytes) throw new Error('Cortez portrait file differs');
      const body = await readFile(path);
      if (!(portrait.src.endsWith('.png') ? body.subarray(0,8).equals(Buffer.from([137,80,78,71,13,10,26,10])) : body[0] === 0xff && body[1] === 0xd8 && body[2] === 0xff) ||
          createHash('sha256').update(body).digest('hex') !== portrait.sha256) throw new Error('Cortez portrait digest or type differs');
    }
  }
  const keenesburg = validateKeenesburgMotion(JSON.parse(contents.get("public/keenesburg-motion-v1.json") ?? ""));
  validateKeenesburgResearchBinding(keenesburg, research);
  const garden = validateGardenMotion(JSON.parse(contents.get("public/garden-city-motion-v1.json") ?? ""));
  validateGardenResearchBinding(garden, research);
  const gardenProof = JSON.parse(await readFile(join(repositoryRoot, "fixtures/atlas/portrait-sources/garden-city-2026.json"), "utf8"));
  validateGardenPortraitProof(gardenProof);
  const paths = Object.values(gardenPortraits).map((portrait) => join(root, portrait.src));
  const present = await Promise.all(paths.map(async (path) => {
    try { await lstat(path); return true; } catch (error) { if (error.code === "ENOENT") return false; throw error; }
  }));
  if (garden.state === "available" || present.some(Boolean)) {
    for (const [i, portrait] of Object.values(gardenPortraits).entries()) {
      if (!present[i]) throw new Error("Garden City portrait set incomplete");
      const meta = await lstat(paths[i]);
      if (!meta.isFile() || meta.isSymbolicLink() || meta.size !== portrait.image_bytes || meta.size > 2_000_000)
        throw new Error("Garden City portrait file is unsafe or wrong size");
      const body = await readFile(paths[i]);
      if (body[0] !== 0xff || body[1] !== 0xd8 || body[2] !== 0xff ||
          createHash("sha256").update(body).digest("hex") !== portrait.image_sha256)
        throw new Error("Garden City portrait digest or type differs");
    }
  }
  const people = research.records.flatMap((item) => [
    ...(item.people_context ?? []),
    ...(item.actions ?? []).flatMap((motion) => motion.votes),
  ]);
  const portraits = new Map(people.filter((person) => person.portrait).map((person) => [person.portrait.src, person.portrait]));
  const proof = JSON.parse(await readFile(join(repositoryRoot, "fixtures/atlas/portrait-sources/arapahoe-2022-budget-proof.json"), "utf8"));
  const expectedUrl = "https://cms3.revize.com/revize/arapahoe/Finance/Complete%20Budget%20Document/2023%20Recommended%20Budget.pdf";
  if (proof.source_url !== expectedUrl || proof.source_page !== 7 ||
      proof.source_sha256 !== "dd6476e64941674e310110b375d90dcf9f81ef904f0866183337b2b0fb867c43" ||
      proof.page_fixture !== "fixtures/atlas/portrait-sources/arapahoe-2022-budget-page.pdf" ||
      proof.page_fixture_sha256 !== "23fc3217eaa1024751d62fde024dcf73f9f0ba6560fb4f86d7df1534f9fc6777" ||
      !Array.isArray(proof.portraits) || proof.portraits.length !== 5) {
    throw new Error("invalid Arapahoe portrait source proof");
  }
  const page = join(repositoryRoot, proof.page_fixture);
  const pageMeta = await lstat(page);
  if (!pageMeta.isFile() || pageMeta.isSymbolicLink() || pageMeta.size > 2_000_000 ||
      createHash("sha256").update(await readFile(page)).digest("hex") !== proof.page_fixture_sha256) {
    throw new Error("Arapahoe source page digest mismatch");
  }
  const arapahoe2024 = JSON.parse(await readFile(join(repositoryRoot, "fixtures/atlas/portrait-sources/arapahoe-2024-budget-proof.json"), "utf8"));
  if (arapahoe2024.source_url !== "https://cms3.revize.com/revize/arapahoe/Finance/Complete%20Budget%20Document/2024%20Adopted%20Budget.pdf" ||
      arapahoe2024.source_page !== 61 || arapahoe2024.source_sha256 !== "4e5366320ab2e190c4f5ab080669458566ed7a82fc666da85330db7588528f0c" ||
      arapahoe2024.page_fixture !== "fixtures/atlas/portrait-sources/arapahoe-2024-budget-page.pdf" ||
      arapahoe2024.page_fixture_sha256 !== "53e344b2f0f9747f1f1461e38934eafea675c1c81963c0901629411e99aa6531" ||
      arapahoe2024.alias_source_url !== "https://www.jessicaforcommissioner.com/home-1" ||
      arapahoe2024.alias_source_sha256 !== "0ba051c1be247ce00d6ea2aaa282af5707fc7f2fd001aa0ed4fb26fc2fd90712" ||
      !Array.isArray(arapahoe2024.portraits) || arapahoe2024.portraits.length !== 2) throw new Error("invalid 2024 Arapahoe portrait proof");
  const arapahoe2024Page = join(repositoryRoot, arapahoe2024.page_fixture);
  const arapahoe2024Meta = await lstat(arapahoe2024Page);
  if (!arapahoe2024Meta.isFile() || arapahoe2024Meta.isSymbolicLink() || arapahoe2024Meta.size > 2_000_000 ||
      createHash("sha256").update(await readFile(arapahoe2024Page)).digest("hex") !== arapahoe2024.page_fixture_sha256) {
    throw new Error("2024 Arapahoe source page digest mismatch");
  }
  const littleton = JSON.parse(await readFile(join(repositoryRoot, "fixtures/atlas/portrait-sources/littleton-2023-report-proof.json"), "utf8"));
  if (littleton.source_url !== "https://www.littletonco.gov/files/sharedassets/public/v/1/communications/littleton-reports/2023-littleton-reports/2023-02-feb-littleton-report.pdf" ||
      littleton.source_page !== 3 || littleton.source_sha256 !== "34a8dc30a20a37c23179b122722175be6cf3c253c825009db6fbdeb4cff6ea7e" ||
      littleton.page_fixture !== "fixtures/atlas/portrait-sources/littleton-2023-report-page.pdf" ||
      littleton.page_fixture_sha256 !== "b6be37f13111fc566f28900396619154179a4dce635529b538c67fe1a83c6ada" ||
      !Array.isArray(littleton.portraits) || littleton.portraits.length !== 7) {
    throw new Error("invalid Littleton portrait source proof");
  }
  const littletonPage = join(repositoryRoot, littleton.page_fixture);
  const littletonMeta = await lstat(littletonPage);
  if (!littletonMeta.isFile() || littletonMeta.isSymbolicLink() || littletonMeta.size > 2_000_000 ||
      createHash("sha256").update(await readFile(littletonPage)).digest("hex") !== littleton.page_fixture_sha256) {
    throw new Error("Littleton source page digest mismatch");
  }
  const later = JSON.parse(await readFile(join(repositoryRoot, "fixtures/atlas/portrait-sources/littleton-2024-report-proof.json"), "utf8"));
  if (later.source_url !== "https://www.littletonco.gov/files/sharedassets/public/v/1/communications/littleton-reports/2024-littleton-reports/2024-12-dec-jan-littleton-report.pdf" ||
      later.source_page !== 3 || later.source_sha256 !== "f8264f7fc25a40afdc6c34ba41ea1864bbc5ac219f2485f91bf2c6ae9ba7e394" ||
      later.page_fixture !== "fixtures/atlas/portrait-sources/littleton-2024-report-page.pdf" ||
      later.page_fixture_sha256 !== "0bb29b5d44dae80db18632b843b3f97f71a32f525d34df84f072088385ecdf50" ||
      !Array.isArray(later.portraits) || later.portraits.length !== 2) throw new Error("invalid 2024 Littleton portrait proof");
  const laterPage = join(repositoryRoot, later.page_fixture);
  const laterMeta = await lstat(laterPage);
  if (!laterMeta.isFile() || laterMeta.isSymbolicLink() || laterMeta.size > 2_000_000 ||
      createHash("sha256").update(await readFile(laterPage)).digest("hex") !== later.page_fixture_sha256) {
    throw new Error("2024 Littleton source page digest mismatch");
  }
  const commerce = JSON.parse(await readFile(join(repositoryRoot, "fixtures/atlas/portrait-sources/commerce-city-2022-report-proof.json"), "utf8"));
  if (commerce.source_url !== "https://www.c3gov.com/files/assets/city/v/1/fd-finance-department/documents/popular-annual-financial-report/22commpafrbookletr5.pdf" ||
      commerce.source_page !== 2 || commerce.source_sha256 !== "142eb3b0f056a0a6d4fd82113052e5c0b081d800e76a9507c926a877bdb510be" ||
      commerce.page_fixture !== "fixtures/atlas/portrait-sources/commerce-city-2022-report-page.pdf" ||
      commerce.page_fixture_sha256 !== "25d3ae7b31b296a2ac1530d80b0be33c174d91899ae6f8b5efd755a4e4c22ce6" ||
      !Array.isArray(commerce.portraits) || commerce.portraits.length !== 5) throw new Error("invalid Commerce City portrait proof");
  const commercePage = join(repositoryRoot, commerce.page_fixture);
  const commerceMeta = await lstat(commercePage);
  if (!commerceMeta.isFile() || commerceMeta.isSymbolicLink() || commerceMeta.size > 2_000_000 ||
      createHash("sha256").update(await readFile(commercePage)).digest("hex") !== commerce.page_fixture_sha256) {
    throw new Error("Commerce City source page digest mismatch");
  }
  const denver2019 = JSON.parse(await readFile(join(repositoryRoot, "fixtures/atlas/portrait-sources/denver-2019-budget-proof.json"), "utf8"));
  if (denver2019.source_url !== "https://www.denvergov.org/content/dam/denvergov/Portals/344/documents/Budget/2019/2019_Budget%20Book_V1-OnlineVersion-compressed.pdf" ||
      denver2019.source_page !== 6 || denver2019.source_sha256 !== "95c42fc5f567b3e4527166ff502b2e6145fce0a6bf5f8949d4df8ebc46b03fa2" ||
      denver2019.page_fixture !== "fixtures/atlas/portrait-sources/denver-2019-budget-page.pdf" ||
      denver2019.page_fixture_sha256 !== "1861dddc8b95d0d7718bd6152a172d4cd1a5fc11e11fdad7cac08762add184c9" ||
      denver2019.alias_source_url !== "https://herndon-leadership.squarespace.com/meet-chris" ||
      denver2019.alias_source_sha256 !== "1b12312bdefa5c8638a9c9147bc389009fce2d0f26d722224aa8ca45c32683b0" ||
      !Array.isArray(denver2019.portraits) || denver2019.portraits.length !== 10) throw new Error("invalid 2019 Denver portrait proof");
  const denver2020 = JSON.parse(await readFile(join(repositoryRoot, "fixtures/atlas/portrait-sources/denver-2020-budget-proof.json"), "utf8"));
  if (denver2020.source_url !== "https://www.denvergov.org/content/dam/denvergov/Portals/344/documents/Budget/2020/2020BudgetVolume1.pdf" ||
      denver2020.source_page !== 6 || denver2020.source_sha256 !== "dae887ea080560ece3eae5430dbe80918b2befc548b3aa291f480acdbfe6e7e1" ||
      denver2020.page_fixture !== "fixtures/atlas/portrait-sources/denver-2020-budget-page.pdf" ||
      denver2020.page_fixture_sha256 !== "4dc327169778d106dc9f88c2902aacbe69740bf7dd564b2cbef96942fa06368e" ||
      denver2020.portrait?.person_id !== 472 || denver2020.portrait?.name !== "Candi CdeBaca" ||
      denver2020.portrait?.image_index !== 14 || denver2020.portrait?.original_sha256 !== "ae31c6ea93f266454d497e25c390bc4c8e34a09a1df9fcab06cb97e53086d9d0" ||
      denver2020.portrait?.sha256 !== "d81c9f4e4d3af48a6629196c5693a8a8a6b3f2a2362336a4c46c1994e68c771e") throw new Error("invalid 2020 Denver portrait proof");
  for (const denver of [denver2019, denver2020]) {
    const pagePath = join(repositoryRoot, denver.page_fixture);
    const meta = await lstat(pagePath);
    if (!meta.isFile() || meta.isSymbolicLink() || meta.size > 2_000_000 ||
        createHash("sha256").update(await readFile(pagePath)).digest("hex") !== denver.page_fixture_sha256) {
      throw new Error("Denver source page digest mismatch");
    }
  }
  for (const record of research.records) for (const motion of record.actions ?? []) for (const vote of motion.votes) {
    if (vote.portrait?.source_kind !== "embedded_pdf") continue;
    assertEmbeddedPortraitMatches(record, vote, { proof, arapahoe2024, littleton, later, commerce, denver2019, denver2020 });
  }
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

// The landing page is separately served at /; index.html retains atlas deep links.
try {
  const home = await readFile(join(root, "home.html"), "utf8");
  await readFile(join(root, "home.css"));
  for (const target of ["./home.css", "./index.html", "./research.html", "./decisions/co-colorado-springs-25-581-2025-11-25.html", "./decisions/co-denver-25-0413-2025-05-05.html"]) {
    if (!home.includes(`href="${target}"`)) failures.push(`home.html: missing link ${target}`);
  }
  if (!home.includes("Coverage is incomplete")) failures.push("home.html: missing coverage limitation");
} catch (error) { failures.push(`home page: ${error.message}`); }

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
