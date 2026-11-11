import { PanopticonClient, startSnapshotRefresh } from "./api.js";
import { portraitForRecordedVote } from "./portraits/portraits.js";

const params = new URLSearchParams(location.search);
const casePath = /\/decisions\/([a-z0-9-]+)\.html$/.exec(location.pathname ?? "");
export function decisionModeFromLocation(pathname, search) {
  const staticCase = /\/decisions\/[a-z0-9-]+\.html$/.test(pathname ?? "");
  const query = new URLSearchParams(search ?? "");
  return { demo: !staticCase && query.get("demo") === "1", preview: !staticCase && query.get("preview") === "1" };
}
const { demo, preview } = decisionModeFromLocation(location.pathname, location.search);
const rootPrefix = casePath ? "../" : "./";
const page = document.querySelector("#decision-page");

export function decisionIdFromLocation(pathname, search) {
  return /\/decisions\/([a-z0-9-]+)\.html$/.exec(pathname ?? "")?.[1]
    ?? new URLSearchParams(search ?? "").get("decision");
}

function decisionPageLink(entity) {
  const slug = entity.slug ?? entity.id;
  if (!demo && !preview && entity.share_url?.endsWith(`/decisions/${slug}.html`)) {
    return `${rootPrefix}decisions/${encodeURIComponent(slug)}.html`;
  }
  return `${rootPrefix}decision.html?decision=${encodeURIComponent(slug)}${demo ? "&demo=1" : preview ? "&preview=1" : ""}`;
}

function node(tag, className = "", value = "") {
  const item = document.createElement(tag);
  if (className) item.className = className;
  if (value) item.textContent = String(value);
  return item;
}

function section(title) {
  const panel = node("section", "panel");
  panel.append(node("h2", "", title));
  return panel;
}

function publicLink(url, label) {
  const link = node("a", "", label);
  link.href = url;
  link.target = "_blank";
  link.rel = "noopener noreferrer";
  return link;
}

function atlasLink(entity, label = entity.name) {
  const link = node("a", "", label);
  link.href = `${rootPrefix}index.html${demo ? "?demo=1" : preview ? "?preview=1" : ""}#entity=${encodeURIComponent(entity.id)}`;
  return link;
}

function evidenceIds(entity) {
  return [...new Set([
    ...(entity.attributes ?? []).map((item) => item.evidence_id),
    ...(entity.relationships ?? []).map((item) => item.evidence_id),
    ...(entity.timeline ?? []).map((item) => item.evidence_id),
    ...(entity.votes ?? []).map((item) => item.evidence_id),
    ...(entity.consent_block?.roll_call ?? []).map((item) => item.evidence_id),
    entity.vote_gap?.evidence_id,
    ...(entity.technology_context ?? []).map((item) => item.evidence_id),
    entity.consent_block?.motion_evidence_id,
    entity.consent_block?.motion_title_evidence_id,
    entity.consent_block?.motion_action_evidence_id,
    entity.consent_block?.motion_outcome_evidence_id,
    entity.consent_block?.item_consent_evidence_id,
  ].filter(Boolean))];
}

export function buildDecisionModel(snapshot, requestedId) {
  const entities = new Map(snapshot.entities.map((item) => [item.id, item]));
  const sources = new Map(snapshot.sources.map((item) => [item.id, item]));
  const evidence = new Map(snapshot.evidence.map((item) => [item.id, item]));
  const decision = entities.get(requestedId) ?? snapshot.entities.find((item) => item.slug === requestedId);
  if (!decision || String(decision.type).toUpperCase() !== "DECISION") return null;

  const links = (decision.relationships ?? []).filter((relation) => relation.type !== "RECORDED_VOTE").map((relation) => ({
    relation,
    target: entities.get(relation.target_entity_id),
    evidence: evidence.get(relation.evidence_id),
  })).filter((item) => item.target && item.evidence);
  const contractTypes = new Set(["CONTRACT", "AWARD", "AGREEMENT"]);
  const contracts = links.filter(({ target }) => contractTypes.has(String(target.type).toUpperCase()));
  const votes = decision.votes ?? [];
  const technologies = (decision.technology_context ?? []).map((item) => ({
    name: item.name,
    context: item.context,
    evidence: evidence.get(item.evidence_id),
  })).filter((item) => item.name && item.evidence);
  const unknowns = (decision.not_established ?? decision.unknowns ?? [])
    .map((item) => typeof item === "string" ? item : item?.detail ?? item?.text ?? item?.description)
    .filter(Boolean);
  if (!contracts.length && !decision.consent_block && !unknowns.some((item) => /contract|award|vendor/i.test(item))) {
    unknowns.unshift("A specific vendor contract or award connected to this decision is not established in the source record.");
  }
  if (!votes.length && !unknowns.some((item) => /recorded vote|individual vote|signature/i.test(item))) {
    unknowns.push("An individual recorded vote or signature is not established in this dataset.");
  }

  const usedEvidence = new Map();
  for (const id of evidenceIds(decision)) if (evidence.has(id)) usedEvidence.set(id, evidence.get(id));
  const usedSources = new Map();
  for (const id of decision.source_ids ?? []) if (sources.has(id)) usedSources.set(id, sources.get(id));
  for (const item of usedEvidence.values()) if (sources.has(item.source_id)) usedSources.set(item.source_id, sources.get(item.source_id));
  const corrections = [
    ...(decision.corrections ?? []).map((item) => ({ ...item, context: decision.name })),
    ...(snapshot.corrections?.filter((item) => item.entity_id === decision.id) ?? []),
    ...[...usedEvidence.values()].flatMap((record) => (record.corrections ?? []).map((item) => ({ ...item, context: record.claim ?? record.id }))),
    ...(decision.attributes ?? []).flatMap((attribute) => (attribute.corrections ?? []).map((item) => ({ ...item, context: attribute.label ?? "Attribute" }))),
    ...(decision.relationships ?? []).flatMap((relation) => (relation.corrections ?? []).map((item) => ({ ...item, context: relation.label ?? relation.type }))),
    ...votes.flatMap((vote) => (vote.corrections ?? []).map((item) => ({ ...item, context: `${vote.name ?? vote.officeholder ?? "Vote"} vote` }))),
    ...(decision.consent_block?.roll_call ?? []).flatMap((vote) => (vote.corrections ?? []).map((item) => ({ ...item, context: `${vote.name ?? "Voter"} consent motion vote` }))),
    ...(decision.technology_context ?? []).flatMap((item) => (item.corrections ?? []).map((correction) => ({ ...correction, context: item.name ?? "Technology context" }))),
  ];
  return { decision, links, contracts, votes, technologies, unknowns, evidence: [...usedEvidence.values()], sources: [...usedSources.values()], sourceById: sources, entityById: entities, corrections };
}

function addNotice(mode) {
  if (mode === "public") return;
  const label = mode === "demo"
    ? "SYNTHETIC DEMONSTRATION — THESE RECORDS DO NOT DESCRIBE REAL PEOPLE OR CONTRACTS."
    : "PRIVATE EDITORIAL PREVIEW — CASE PENDING RELEASE. DO NOT SHARE AS A PUBLISHED FINDING.";
  page.append(node("div", `notice ${mode === "demo" ? "demo" : ""}`, label));
}

function addMeta(container, label) {
  container.append(node("span", "", label));
}

function renderChain(model) {
  const panel = section("Follow the recorded chain");
  const list = node("ol", "chain");
  const addRelation = ({ relation, target, evidence }) => {
    const item = node("li");
    item.append(node("span", "kind", `${target.type} · ${relation.label ?? relation.type}`));
    const name = node("strong");
    name.append(atlasLink(target));
    item.append(name);
    const detail = node("span", "detail");
    detail.append("Supported by ");
    const jump = node("a", "", "source evidence");
    jump.href = `#evidence-${encodeURIComponent(evidence.id)}`;
    detail.append(jump, ".");
    item.append(detail);
    list.append(item);
  };
  const context = model.links.filter(({ target }) => ["JURISDICTION", "AGENCY"].includes(String(target.type).toUpperCase()));
  for (const item of context) addRelation(item);
  const first = node("li");
  first.append(node("span", "kind", "Decision"), node("strong", "", model.decision.name));
  if (model.decision.description) first.append(node("span", "detail", model.decision.description));
  list.append(first);
  for (const item of model.contracts) addRelation(item);
  if (!model.contracts.length) {
    const item = node("li");
    item.append(node("span", "kind", model.decision.consent_block ? "Executed agreement" : "Contract or award"), node("strong", "empty", "Not established"));
    list.append(item);
  }
  for (const item of model.links.filter((link) => !context.includes(link) && !model.contracts.includes(link) && String(link.target.type).toUpperCase() !== "TECHNOLOGY")) addRelation(item);
  panel.append(list);
  return panel;
}

function renderUnknowns(model) {
  const panel = section("What is not established");
  const list = node("ul", "unknown-list");
  for (const item of model.unknowns) list.append(node("li", "", item));
  if (!model.unknowns.length) list.append(node("li", "", "No unresolved links are listed in this published record."));
  panel.append(list);
  if (model.decision.unresolved_question) panel.append(node("p", "empty", `Open question: ${model.decision.unresolved_question}`));
  return panel;
}

function renderTechnology(model) {
  const technologies = model.technologies.length ? model.technologies : model.links
    .filter(({ target }) => String(target.type).toUpperCase() === "TECHNOLOGY")
    .map(({ target, evidence }) => ({ name: target.name, context: "Named in supporting materials", evidence }));
  if (!technologies.length) return null;
  const panel = section("Technology in the cited record");
  panel.append(node("p", "", model.contracts.length
    ? "An evidence-backed agreement link is shown for this decision. The source card states what that link establishes; execution, deployment, and payment require separate evidence."
    : model.decision.consent_block
      ? "The approved consent item names these systems. Signed execution, deployment, and payment require separate evidence."
      : "These systems appear in the cited materials. Their mention alone does not establish a contract or award from this decision."));
  const list = node("ul", "source-list");
  for (const { name, context, evidence } of technologies) {
    const item = node("li");
    item.append(node("strong", "", name), node("span", "detail", ` — ${context}. `));
    const jump = node("a", "", " · Source context ↗");
    jump.href = `#evidence-${encodeURIComponent(evidence.id)}`;
    item.append(jump);
    list.append(item);
  }
  panel.append(list);
  return panel;
}

function renderBrief(model, mode) {
  const panel = section("Evidence at a glance");
  panel.append(node("p", "", model.decision.summary ?? model.decision.description ?? "The decision is indexed from public records."));
  const first = model.evidence[0];
  if (first) {
    const source = model.sourceById.get(first.source_id);
    panel.append(node("p", "locator", `Proof: ${source?.title ?? first.source_id} · ${first.locator}`));
    if (source && mode !== "demo") panel.append(publicLink(source.canonical_url, "Read the source ↗"));
  }
  if (!model.contracts.length) panel.append(node("p", "empty", model.decision.consent_block
    ? "Signed execution and payment: not established by these records."
    : "Specific contract or award: not established."));
  return panel;
}

function initials(name) {
  return String(name).split(/\s+/).filter(Boolean).slice(0, 2).map((part) => part[0]).join("").toUpperCase();
}

function renderVoteCards(model, votes, mode) {
  const list = node("ul", "vote-list");
  for (const vote of votes) {
    const item = node("li", "vote-card");
    const name = vote.name ?? vote.officeholder ?? "Unnamed officeholder";
    const portrait = mode === "demo" ? null : portraitForRecordedVote(vote, model.entityById.get(vote.voter_entity_id));
    const media = node("div", "vote-portrait");
    const fallback = node("span", "vote-initials", initials(name));
    media.append(fallback);
    if (portrait) {
      const img = node("img");
      img.src = `${rootPrefix}${portrait.src.replace(/^\.\//, "")}`;
      img.alt = `Portrait of ${name}`;
      img.loading = "lazy";
      img.decoding = "async";
      img.width = 144;
      img.height = 144;
      img.addEventListener("error", () => img.remove(), { once: true });
      media.append(img);
    }
    const text = node("div", "vote-card-text");
    const heading = node("div", "vote-card-heading");
    const voteValue = vote.vote ?? vote.position;
    const voteKey = String(voteValue ?? "").toLowerCase();
    const voteClass = ["aye", "yes"].includes(voteKey) ? "aye" : ["no", "nay"].includes(voteKey) ? "no" : "";
    heading.append(node("strong", "", name), node("span", `vote-position${voteClass ? ` ${voteClass}` : ""}`, voteValue ?? "Vote unknown"));
    text.append(heading);
    const role = vote.role ?? vote.office_role ?? "not established";
    const term = vote.term ?? vote.office_term ?? "not established";
    const roleTerm = role === "not_established" && term === "not_established" ? "Public role and term: not established" : `Public role: ${role}; term: ${term}`;
    text.append(node("p", "vote-detail", roleTerm));
    if (portrait?.portrait_date === "not_established") text.append(node("p", "vote-detail", "Portrait date: not established"));
    if (portrait?.source_conflict?.state === "UNRESOLVED") {
      text.append(node("p", "vote-detail", portrait.source_conflict.public_note));
    }
    if (!portrait && mode !== "demo") text.append(node("p", "vote-detail", "Official portrait: not yet verified"));
    const links = node("div", "vote-links");
    if (vote.evidence_id) {
      const link = node("a", "", "Vote record ↗");
      link.href = `#evidence-${encodeURIComponent(vote.evidence_id)}`;
      links.append(link);
    }
    if (portrait) {
      links.append(publicLink(`${rootPrefix}${portrait.src.replace(/^\.\//, "")}`, "Open preserved image ↗"));
      links.append(publicLink(portrait.source_url, `Photo source: ${portrait.credit ?? "official source"} ↗`));
    }
    text.append(links);
    item.append(media, text);
    list.append(item);
  }
  return list;
}

function renderVotes(model, mode) {
  const consent = model.decision.consent_block;
  const panel = section(consent ? "Consent agenda vote" : "Recorded votes on this decision");
  if (consent) {
    panel.append(node("p", "vote-note", `This item was included in consent motion ${consent.motion_item_id}, “${consent.motion_title}” (${consent.motion_outcome}). The roll call covers the block of items.`));
    const links = node("p", "vote-links");
    for (const [id, label] of [
      [consent.item_consent_evidence_id, "Item's consent designation ↗"],
      [consent.motion_evidence_id, "Motion item ID ↗"],
      [consent.motion_title_evidence_id, "Motion title ↗"],
      [consent.motion_action_evidence_id, "Motion action ↗"],
      [consent.motion_outcome_evidence_id, "Motion outcome ↗"],
    ]) {
      const link = node("a", "", label);
      link.href = `#evidence-${encodeURIComponent(id)}`;
      links.append(link);
    }
    panel.append(links, node("p", "empty", "Individual votes on this item are not established."));
    const rollCall = consent.roll_call ?? [];
    if (rollCall.length) {
      panel.append(node("h3", "", "Recorded vote on the consent motion (context)"));
      panel.append(node("p", "vote-note", "These positions are on the block motion, not on this item alone."));
      panel.append(renderVoteCards(model, rollCall, mode));
    }
    const blockSource = model.sourceById.get(consent.block_vote_source_id);
    if (blockSource && mode !== "demo") panel.append(publicLink(blockSource.canonical_url, "Block roll call source (context) ↗"));
    return panel;
  }
  if (!model.votes.length) {
    panel.append(node("p", "empty", "Individual votes are not established in the checked records."));
    if (model.decision.vote_gap?.evidence_id) {
      const proof = node("a", "", "Empty vote response proof ↗");
      proof.href = `#evidence-${encodeURIComponent(model.decision.vote_gap.evidence_id)}`;
      panel.append(proof);
    }
    const gapSource = model.sourceById.get(model.decision.vote_gap?.checked_vote_source_id);
    if (gapSource && mode !== "demo") panel.append(publicLink(gapSource.canonical_url, "Checked vote source ↗"));
    return panel;
  }
  panel.append(node("p", "vote-note", "Recorded positions come from the cited roll call. Official portraits appear where a source is available."));
  panel.append(renderVoteCards(model, model.votes, mode));
  return panel;
}

function renderEvidence(model, mode) {
  const panel = section("Evidence and exact locators");
  if (!model.evidence.length) panel.append(node("p", "empty", "No evidence is attached to this decision. No unsupported connection is shown."));
  for (const item of model.evidence) {
    const source = model.sourceById.get(item.source_id);
    const card = node("article", "evidence-card");
    card.id = `evidence-${item.id}`;
    card.append(node("h3", "", item.claim ?? "Recorded claim"));
    if (item.quote) card.append(node("blockquote", "", item.quote));
    const verificationLabel = mode === "demo" ? "SYNTHETIC DEMO" : item.review_state === "SOURCE_VERIFIED" ? "SOURCE VERIFIED" : "PENDING CASE RELEASE";
    card.append(node("p", "review", `${verificationLabel} · ${source?.authority ?? item.authority ?? "SOURCE"}`));
    card.append(node("p", "locator", `${source?.title ?? item.source_id} · ${item.locator}`));
    card.append(node("p", "digest", `Preserved SHA-256: ${item.sha256} · Verification binding: ${item.verification_digest}`));
    if (source && mode !== "demo") card.append(publicLink(source.canonical_url, "Open original public source ↗"));
    if (mode === "demo") card.append(node("p", "empty", "Synthetic source · demonstration only"));
    panel.append(card);
  }
  return panel;
}

function renderSources(model, mode) {
  const panel = section("Source records");
  const list = node("ul", "source-list");
  for (const source of model.sources) {
    const item = node("li");
    if (mode === "demo") item.append(node("span", "", `${source.title} · synthetic demonstration source`));
    else item.append(publicLink(source.canonical_url, source.title));
    item.append(node("span", "detail", ` · ${source.authority ?? "Public source"} · SHA-256 ${source.sha256}`));
    list.append(item);
  }
  if (!model.sources.length) list.append(node("li", "empty", "No sources published for this decision."));
  panel.append(list);
  return panel;
}

function renderCorrections(model) {
  const panel = section("Corrections and revisions");
  const list = node("ul", "correction-list");
  for (const item of model.corrections) list.append(node("li", "", `${item.date ?? item.at ?? "Date unknown"} — ${item.context ? `${item.context}: ` : ""}${item.summary ?? item.reason ?? item.state ?? "Correction recorded"}`));
  if (!model.corrections.length) list.append(node("li", "", "No published corrections are recorded for this decision."));
  panel.append(list);
  return panel;
}

function renderDecision(model, snapshot, mode) {
  page.replaceChildren();
  addNotice(mode);
  page.append(node("p", "eyebrow", "Public decision · source-grounded record"));
  page.append(node("h1", "", model.decision.name));
  if (model.decision.description) page.append(node("p", "lead", model.decision.description));
  const meta = node("div", "meta");
  const caseRelease = snapshot.case_releases?.find((item) => item.case_id === model.decision.slug)?.release ?? snapshot.release_review;
  addMeta(meta, `Case release: ${mode === "preview" ? "PENDING" : mode === "demo" ? "SYNTHETIC DEMO" : caseRelease?.state === "SOURCE_VERIFIED" ? "SOURCE VERIFIED" : "APPROVED"}`);
  if (mode === "public") addMeta(meta, "Sources: VERIFIED");
  const date = model.decision.date ?? model.decision.timeline?.[0]?.date;
  if (date) addMeta(meta, `Decision date: ${date}`);
  if (snapshot.status?.last_publish) addMeta(meta, `Snapshot: ${snapshot.status.last_publish}`);
  page.append(meta);

  const grid = node("div", "grid");
  const main = node("div");
  main.append(renderChain(model), renderVotes(model, mode));
  const technology = renderTechnology(model);
  if (technology) main.append(technology);
  main.append(renderEvidence(model, mode), renderCorrections(model));
  const sidebar = node("div");
  sidebar.append(renderBrief(model, mode), renderUnknowns(model), renderSources(model, mode));
  grid.append(main, sidebar);
  page.append(grid);
  document.title = `${model.decision.name} | PANOPTICON.FAIL`;
  if (mode === "public" && !casePath) {
    const canonical = document.querySelector('link[rel="canonical"]') ?? node("link");
    canonical.rel = "canonical";
    const slug = model.decision.slug;
    const shareUrl = model.decision.share_url;
    try {
      const target = new URL(shareUrl);
      canonical.href = target.protocol === "https:" && target.pathname === `/decisions/${slug}.html` && !target.search && !target.hash
        ? target.href : `${location.origin}${location.pathname}`;
    } catch {
      canonical.href = `${location.origin}${location.pathname}`;
    }
    if (!canonical.isConnected) document.head.append(canonical);
  }
}

async function main() {
  const client = new PanopticonClient({
    demo, preview,
    publicUrl: `${rootPrefix}public/public-snapshot.json`,
    demoUrl: `${rootPrefix}mock/public-snapshot.json`,
    previewUrl: `${rootPrefix}preview-snapshot.json`,
  });
  await client.bootstrap();
  startSnapshotRefresh(client);
  const snapshot = client.snapshot;
  const id = decisionIdFromLocation(location.pathname, location.search);
  if (!id) {
    page.replaceChildren();
    addNotice(client.mode);
    page.append(node("h1", "", "Published decisions"));
    const decisions = snapshot.entities.filter((item) => String(item.type).toUpperCase() === "DECISION");
    if (!decisions.length) page.append(node("p", "empty", "No source-verified cases have been released yet."));
    const list = node("ul", "source-list");
    for (const item of decisions) {
      const link = node("a", "", item.name);
      link.href = decisionPageLink(item);
      const li = node("li");
      li.append(link);
      list.append(li);
    }
    page.append(list);
    return;
  }
  const model = buildDecisionModel(snapshot, id);
  if (!model) throw new Error("This decision is not in the selected published dataset.");
  renderDecision(model, snapshot, client.mode);
}

if (page) main().catch((error) => {
  page.replaceChildren(node("h1", "", "Decision unavailable"), node("p", "empty", error.message));
});
