import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { votePortraits } from "../portraits/portraits.js";
import { validateSnapshot } from "../api.js";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const read = (path) => readFile(join(root, path));
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const snapshot = JSON.parse(await read("web/panopticon-terminal/public/public-snapshot.json"));
const card = (await read("docs/launch/commerce-city-2026.md")).toString("utf8");
const draft = (await read("docs/launch/commerce-city-x-draft.md")).toString("utf8");
const readme = (await read("README.md")).toString("utf8");
const graphic = await read("docs/launch/commerce-city-2026-social.png");
const graphicSource = (await read("docs/launch/commerce-city-2026-social.svg")).toString("utf8");
validateSnapshot(snapshot);
assert.equal(snapshot.status.state, "released");
assert.equal(snapshot.preview, false);
assert.equal(snapshot.demo, false);

const cases = [
  {
    slug: "co-commerce-city-ord-2755-2026-04-06",
    matter: "Ord 2755",
    eventItemId: 70384,
    votesSource: "co-commerce-2026-ord2755-votes",
    votesPath: "fixtures/co-commerce/votes-70384-ord-2755.json",
    titleNeedles: ["FLOCK GROUP, INC.", "LICENSE PLATE RECOGNITION CAMERAS, DRONES"],
    unknownKind: "contract_execution",
  },
  {
    slug: "co-commerce-city-ord-2756-2026-04-06",
    matter: "Ord 2756",
    eventItemId: 70385,
    votesSource: "co-commerce-2026-ord2756-votes",
    votesPath: "fixtures/co-commerce/votes-70385-ord-2756.json",
    titleNeedles: ["$608,900", "FLOCK EXPANSION PROJECT"],
    unknownKind: "contract_award",
  },
];
const sources = new Map(snapshot.sources.map((item) => [item.id, item]));

async function verifiedFixture(path, sourceId) {
  const record = sources.get(sourceId);
  assert.match(record?.sha256 ?? "", /^[a-f0-9]{64}$/);
  const bytes = await read(path);
  assert.equal(sha256(bytes), record.sha256, sourceId + " digest changed");
  return bytes;
}

const event = JSON.parse(await verifiedFixture(
  "fixtures/co-commerce/event-4626-2026-04-06.json", "co-commerce-2026-event"));
await verifiedFixture("fixtures/co-commerce/minutes-2026-04-06.pdf",
  "co-commerce-2026-minutes");
assert.match(event.EventDate, /^2026-04-06/);
assert.equal(event.EventBodyName, "City Council");
assert.ok(card.includes(sources.get("co-commerce-2026-minutes").canonical_url));
assert.ok(card.includes("pages 3–4"));
assert.ok(card.includes("do not establish later execution, payment, or deployment"));
assert.ok(card.includes("separate pilot-program amendment that failed 2–7"));
assert.ok(card.includes("$608,900 budget appropriation"));
assert.ok(card.includes("](commerce-city-2026-social.png)"));
assert.ok(draft.includes("](commerce-city-2026-social.png)"));
assert.equal(graphic.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
assert.ok(graphicSource.includes("Two Flock votes. Same 7–2 split."));
assert.ok(graphicSource.includes("$608,900"));
assert.ok(readme.includes("docs/launch/commerce-city-2026.md"));
assert.ok(!card.includes("https://panopticon.fail/"));

let firstVotes;
for (const spec of cases) {
  const decision = snapshot.entities.find((item) =>
    item.type === "DECISION" && item.slug === spec.slug);
  const release = snapshot.case_releases.find((item) => item.case_id === spec.slug);
  assert.ok(decision && release, spec.matter + " is not released");
  assert.equal(release.release.state, "SOURCE_VERIFIED");
  assert.deepEqual(release.release.corrections, []);
  assert.ok(!snapshot.withdrawals?.some((item) => item.case_id === spec.slug));
  assert.equal(decision.status, "SOURCE_VERIFIED");
  assert.equal(decision.vote_record_state, "direct");
  assert.ok(decision.unknowns.some((item) =>
    item.kind === spec.unknownKind && item.state === "not_established"));
  const item = event.EventItems.filter((row) => row.EventItemId === spec.eventItemId);
  assert.equal(item.length, 1);
  assert.equal(item[0].EventItemMatterFile, spec.matter);
  assert.equal(item[0].EventItemActionName, "approved on second & final reading");
  assert.equal(item[0].EventItemPassedFlagName, "Pass");
  for (const phrase of spec.titleNeedles) assert.ok(item[0].EventItemTitle.includes(phrase));

  const official = JSON.parse(await verifiedFixture(spec.votesPath, spec.votesSource));
  assert.equal(official.length, 9);
  assert.equal(decision.votes.length, 9);
  const byName = new Map(decision.votes.map((vote) => [vote.name, vote]));
  assert.equal(byName.size, 9);
  const aye = official.filter((vote) => vote.VoteValueName === "Aye");
  const nay = official.filter((vote) => vote.VoteValueName === "Nay");
  assert.equal(aye.length, 7);
  assert.equal(nay.length, 2);
  for (const vote of official) {
    assert.equal(vote.VoteEventItemId, spec.eventItemId);
    const published = byName.get(vote.VotePersonName);
    assert.equal(published?.vote, vote.VoteValueName);
    assert.ok(published.voter_entity_id.endsWith("_" + vote.VotePersonId));
    assert.ok(card.includes("| " + vote.VotePersonName + " | **" +
      vote.VoteValueName + "** | **" + vote.VoteValueName + "** |"));
    if (firstVotes) {
      assert.equal(firstVotes.get(vote.VotePersonName), vote.VoteValueName,
        "two direct ordinance roll calls differ");
    } else {
      const portrait = votePortraits[published.voter_entity_id];
      assert.equal(portrait?.name, vote.VotePersonName);
      assert.equal(portrait.credit, "City of Commerce City");
      assert.equal(portrait.verification_state, "SOURCE_BYTES_VERIFIED");
      const image = await read("web/panopticon-terminal/" + portrait.src.slice(2));
      assert.equal(sha256(image), portrait.sha256);
      assert.ok(card.includes("../../web/panopticon-terminal/" + portrait.src.slice(2)));
      assert.ok(card.includes(portrait.source_url));
    }
  }
  if (!firstVotes) firstVotes = new Map(official.map((vote) =>
    [vote.VotePersonName, vote.VoteValueName]));
}

const post = draft.split("\n").find((line) => line.startsWith("Commerce City, CO:"));
assert.ok(post && post.length <= 280, "manual X post exceeds 280 characters");
assert.ok(post.includes("separate 7–2 votes"));
assert.ok(post.includes("Who signed, and what was spent or deployed?"));
assert.ok(post.endsWith("https://github.com/H4rmon1c/panopticon-null/blob/main/docs/launch/commerce-city-2026.md"));
assert.ok(!post.includes("https://panopticon.fail/"));
console.log("Commerce City launch card: 2 released decisions, 18 votes, 9 portraits, 4 source digests, short manual X draft verified");
