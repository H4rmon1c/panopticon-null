import { PanopticonClient } from "./api.js";

const params = new URLSearchParams(location.search);
const demo = params.get("demo") === "1";
const preview = params.get("preview") === "1";
const page = document.querySelector("#decision-page");

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
  link.href = `./index.html${demo ? "?demo=1" : preview ? "?preview=1" : ""}#entity=${encodeURIComponent(entity.id)}`;
  return link;
}

function evidenceIds(entity) {
  return [...new Set([
    ...(entity.attributes ?? []).map((item) => item.evidence_id),
    ...(entity.relationships ?? []).map((item) => item.evidence_id),
    ...(entity.timeline ?? []).map((item) => item.evidence_id),
    ...(entity.votes ?? []).map((item) => item.evidence_id),
    ...(entity.technology_context ?? []).map((item) => item.evidence_id),
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
  if (!contracts.length && !unknowns.some((item) => /contract|award|vendor/i.test(item))) {
    unknowns.unshift("A specific vendor contract or award connected to this decision is not established in the reviewed record.");
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
    ...(decision.technology_context ?? []).flatMap((item) => (item.corrections ?? []).map((correction) => ({ ...correction, context: item.name ?? "Technology context" }))),
  ];
  return { decision, links, contracts, votes, technologies, unknowns, evidence: [...usedEvidence.values()], sources: [...usedSources.values()], sourceById: sources, corrections };
}

function addNotice(mode) {
  if (mode === "public") return;
  const label = mode === "demo"
    ? "SYNTHETIC DEMONSTRATION — THESE RECORDS DO NOT DESCRIBE REAL PEOPLE OR CONTRACTS."
    : "PRIVATE EDITORIAL PREVIEW — PENDING REVIEW. DO NOT SHARE AS A PUBLISHED FINDING.";
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
    item.append(node("span", "kind", "Contract or award"), node("strong", "empty", "Not established"));
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
  const panel = section("Technology named in staff materials");
  panel.append(node("p", "", "These systems appear in supporting materials. Their mention does not establish a contract or award from this ordinance."));
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
  if (!model.contracts.length) panel.append(node("p", "empty", "Specific contract or award: not established."));
  return panel;
}

function renderVotes(model) {
  const panel = section("Recorded votes on this ordinance");
  if (!model.votes.length) {
    panel.append(node("p", "empty", "Individual votes are not established in this dataset."));
    return panel;
  }
  const list = node("ul", "source-list");
  for (const vote of model.votes) {
    const item = node("li");
    item.append(node("strong", "", `${vote.name ?? vote.officeholder ?? "Unnamed officeholder"} — ${vote.vote ?? vote.position ?? "Vote unknown"}`));
    const role = vote.role ?? vote.office_role ?? "not established";
    const term = vote.term ?? vote.office_term ?? "not established";
    const roleTerm = role === "not_established" && term === "not_established" ? "Public role and term: not established" : `Public role: ${role}; term: ${term}`;
    const detail = node("span", "detail", ` · ${roleTerm}. `);
    item.append(detail);
    if (vote.evidence_id) {
      const link = node("a", "", "Vote record ↗");
      link.href = `#evidence-${encodeURIComponent(vote.evidence_id)}`;
      item.append(link);
    }
    list.append(item);
  }
  panel.append(list);
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
    card.append(node("p", "review", `${item.review_state ?? "REVIEW UNKNOWN"} · ${source?.authority ?? item.authority ?? "SOURCE"}`));
    card.append(node("p", "locator", `${source?.title ?? item.source_id} · ${item.locator}`));
    card.append(node("p", "digest", `Preserved SHA-256: ${item.sha256} · Review binding: ${item.review_bound_digest}`));
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
  addMeta(meta, `Review: ${mode === "preview" ? "PENDING PUBLICATION" : mode === "demo" ? "SYNTHETIC DEMO" : "PUBLISHED"}`);
  if (model.decision.status) addMeta(meta, model.decision.status);
  const date = model.decision.date ?? model.decision.timeline?.[0]?.date;
  if (date) addMeta(meta, `Decision date: ${date}`);
  if (snapshot.status?.last_publish) addMeta(meta, `Snapshot: ${snapshot.status.last_publish}`);
  page.append(meta);

  const grid = node("div", "grid");
  const main = node("div");
  main.append(renderChain(model), renderVotes(model));
  const technology = renderTechnology(model);
  if (technology) main.append(technology);
  main.append(renderEvidence(model, mode), renderCorrections(model));
  const sidebar = node("div");
  sidebar.append(renderBrief(model, mode), renderUnknowns(model), renderSources(model, mode));
  grid.append(main, sidebar);
  page.append(grid);
  document.title = `${model.decision.name} | PANOPTICON.FAIL`;
  if (mode === "public") {
    const canonical = node("link");
    canonical.rel = "canonical";
    canonical.href = `${location.origin}${location.pathname}?decision=${encodeURIComponent(model.decision.slug ?? model.decision.id)}`;
    document.head.append(canonical);
  }
}

async function main() {
  const client = new PanopticonClient({ demo, preview });
  await client.bootstrap();
  const snapshot = client.snapshot;
  const id = params.get("decision");
  if (!id) {
    page.replaceChildren();
    addNotice(client.mode);
    page.append(node("h1", "", "Published decisions"));
    const decisions = snapshot.entities.filter((item) => String(item.type).toUpperCase() === "DECISION");
    if (!decisions.length) page.append(node("p", "empty", "No reviewed decisions have been published yet."));
    const list = node("ul", "source-list");
    for (const item of decisions) {
      const link = node("a", "", item.name);
      link.href = `./decision.html?decision=${encodeURIComponent(item.slug ?? item.id)}${demo ? "&demo=1" : preview ? "&preview=1" : ""}`;
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
