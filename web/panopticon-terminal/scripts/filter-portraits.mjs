import { readFile, readdir, unlink, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { votePortraits } from "../portraits/portraits.js";
import { lookupPortrait } from "../portraits/lookup.js";
import { validateGardenMotion, validateGardenResearchBinding } from "../garden-motion-api.js";
import { gardenPortraits } from "../garden-portraits-api.js";

export function selectPublishedPortraits(snapshot, registry) {
  if (snapshot.schema_version !== "pnull-public-snapshot-v1" || snapshot.demo || snapshot.preview ||
      snapshot.status?.state !== "released" || !Array.isArray(snapshot.entities)) {
    throw new Error("portrait filtering requires a released public snapshot");
  }
  const entities = new Map(snapshot.entities.map((entity) => [entity.id, entity]));
  const kept = {};
  for (const decision of snapshot.entities.filter((entity) => entity.type === "DECISION")) {
    for (const vote of [...(decision.votes ?? []), ...(decision.consent_block?.roll_call ?? [])]) {
      const id = vote.voter_entity_id;
      if (!Object.hasOwn(registry, id)) continue;
      const portrait = lookupPortrait(registry, vote, entities.get(id));
      if (!portrait) throw new Error(`released voter portrait identity differs: ${id}`);
      kept[id] = portrait;
    }
  }
  return kept;
}

export async function filterGardenPortraits(stage, rawGarden, research) {
  const garden = validateGardenMotion(rawGarden);
  validateGardenResearchBinding(garden, research);
  const allowed = new Set(Object.values(gardenPortraits).map((portrait) => portrait.image_sha256 + ".jpg"));
  const entries = await readdir(resolve(stage, "garden-portraits"), { withFileTypes: true });
  if (garden.state === "available" && entries.length !== allowed.size) throw new Error("Garden City portrait set incomplete");
  for (const entry of entries) {
    if (!entry.isFile() || !allowed.has(entry.name)) throw new Error(`unexpected Garden City portrait asset: ${entry.name}`);
    if (garden.state !== "available") await unlink(resolve(stage, "garden-portraits", entry.name));
  }
}

async function main() {
  const stage = resolve(process.argv[2] ?? ".");
  const snapshot = JSON.parse(await readFile(resolve(stage, "public/public-snapshot.json"), "utf8"));
  const kept = selectPublishedPortraits(snapshot, votePortraits);
  const keptPaths = new Set();
  for (const [id, portrait] of Object.entries(kept)) {
    if (!/^\.\/portraits\/[a-z0-9-]+\.(?:jpe?g|png|webp)$/.test(portrait.src)) {
      throw new Error(`unsafe released portrait path: ${id}`);
    }
    keptPaths.add(portrait.src);
  }
  for (const entry of await readdir(resolve(stage, "portraits"), { withFileTypes: true })) {
    const path = `./portraits/${entry.name}`;
    if (entry.name === "portraits.js" || entry.name === "lookup.js" || keptPaths.has(path)) continue;
    if (!entry.isFile()) throw new Error(`unexpected portrait asset: ${entry.name}`);
    await unlink(resolve(stage, "portraits", entry.name));
  }
  const garden = JSON.parse(await readFile(resolve(stage, "public/garden-city-motion-v1.json"), "utf8"));
  const research = JSON.parse(await readFile(resolve(stage, "public/research-signals.json"), "utf8"));
  await filterGardenPortraits(stage, garden, research);
  const source = `import { lookupPortrait } from "./lookup.js";\n` +
    `export const votePortraits = Object.freeze(${JSON.stringify(kept, null, 2)});\n` +
    `export function portraitForRecordedVote(vote, voter) {\n` +
    `  return lookupPortrait(votePortraits, vote, voter);\n}\n`;
  await writeFile(resolve(stage, "portraits/portraits.js"), source);
  console.log(`included ${Object.keys(kept).length} portraits for released recorded voters`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
