function updateCivicContext(entity = currentState().selected) {
  if (!entity) return;
  const snapshot = currentState();
  atlas?.setSelected(entity.id);
  atlas?.setData(snapshot.entities ?? []);
  atlas?.setLayers(snapshot.activeLayers ?? new Set());
  atlas?.setView(snapshot.view === "network" ? "connections" : "place");

  const why = civicWhy(entity);
  const summary = civicSummary(entity, snapshot);
  const unknowns = civicUnknowns(entity, snapshot);

  const whyText = civic$("#civic-why-text");
  if (whyText) whyText.textContent = why;
  const brief = civic$("#civic-brief-text");
  if (brief) brief.textContent = summary;
  const proof = civic$("#civic-brief-proof");
  if (proof) proof.textContent = `${Number(entity.source_count ?? entity.source_ids?.length ?? 0)} public sources · ${Number(entity.relation_count ?? entity.relationships?.length ?? 0)} documented connections · unknowns shown, not guessed.`;

  const list = civic$("#civic-unknown-list");
  if (list) {
    list.replaceChildren();
    unknowns.forEach((text) => {
      const item = document.createElement("li");
      item.textContent = text;
      list.append(item);
    });
  }

  const entityLocation = entity.geo?.label;
  if (entityLocation && !/approximate/i.test(entityLocation)) {
    const clean = entityLocation.replace(/\s+demonstration\s+(region|headquarters|network office)/i, "").trim();
    if (clean) place = clean.toUpperCase();
  }
  updatePlaceLabel();
}

function civicWhy(entity) {
  const type = String(entity.type ?? "").toUpperCase();
  return STAKES[type] ?? "This record matters because it connects a public claim to the organizations, decisions, money, projects, and sources around it.";
}

function civicSummary(entity, snapshot) {
  const type = TYPE_META[String(entity.type ?? "").toUpperCase()]?.title?.toLowerCase() ?? "public record";
  const relations = (entity.relationships ?? [])
    .slice(0, 4)
    .map((item) => snapshot.byId?.get?.(item.target_entity_id)?.name ?? item.target_entity_id)
    .filter(Boolean);
  const latest = [...(entity.timeline ?? [])].sort((a, b) => String(b.date).localeCompare(String(a.date)))[0];
  const relationText = relations.length
    ? ` Records connect it to ${humanList(relations)}.`
    : " No documented connections are published in this dataset yet.";
  const latestText = latest?.title ? ` Latest published change: ${latest.title}.` : "";
  return `${entity.name} is indexed as a ${type}.${relationText}${latestText}`;
}

function civicUnknowns(entity, snapshot) {
  const attributes = entity.attributes ?? [];
  const relationships = entity.relationships ?? [];
  const timeline = entity.timeline ?? [];
  const targetTypes = relationships.map((item) => snapshot.byId?.get?.(item.target_entity_id)?.type).filter(Boolean);
  const unknowns = [];

  if (!attributes.some((item) => /\$|cost|amount|price|award|value|spend/i.test(`${item.label} ${item.value}`))) {
    unknowns.push("No public dollar amount is linked to this record yet.");
  }
  if (!targetTypes.some((type) => /PERSON|OFFICIAL|OFFICEHOLDER/i.test(type))) {
    unknowns.push("No named decision-maker in a documented public role is linked yet.");
  }
  if (!timeline.some((item) => /VOTE|MEETING|ORDINANCE|HEARING/i.test(`${item.type} ${item.title}`))) {
    unknowns.push("No vote, hearing, or meeting record is linked yet.");
  }
  if (!attributes.some((item) => /tax|incentive|subsid|abatement/i.test(`${item.label} ${item.value}`))) {
    unknowns.push("No public incentive or tax-abatement record is linked yet.");
  }
  return unknowns.slice(0, 4);
}

function humanList(values) {
  if (values.length < 2) return values[0] ?? "";
  if (values.length === 2) return `${values[0]} and ${values[1]}`;
  return `${values.slice(0, -1).join(", ")}, and ${values.at(-1)}`;
}

function translateActivityLabels() {
  civic$$(".activity-item em").forEach((label) => {
    const raw = label.dataset.rawType ?? label.textContent.trim();
    label.dataset.rawType = raw;
    label.title = raw;
    label.textContent = ACTIVITY_LABELS[raw] ?? raw.replaceAll("_", " ");
  });
}

function rewriteCommands() {
  const items = civic$$(".command-item");
  const copy = [
    ["Search the public record", "Places, agencies, companies, contracts, votes, projects, and sources"],
    ["Return to place view", "Open the selected community and its public-power map"],
    ["Trace documented connections", "Follow ownership, money, decisions, projects, and infrastructure"],
    ["Read the proof", "Open exact source text, locator, retrieval time, and digest"],
    ["Play record history", "Move through changes in the published record"],
    ["Cycle information density", "Public, dense, and terminal workspaces"],
    ["Plain-language reading", "Summarize what the public record establishes"],
    ["Source-first reading", "Emphasize documents and evidence"],
    ["Change-first reading", "Highlight new, revised, and contradicted records"],
  ];
  items.forEach((item, index) => {
    const [title, description] = copy[index] ?? [];
    if (!title) return;
    const bold = item.querySelector("b");
    const small = item.querySelector("small");
    if (bold) bold.textContent = title;
    if (small) small.textContent = description;
  });
}

function updateReadingMode(sensor) {
  const labels = {
    record: "PLAIN-LANGUAGE VIEW",
    night: "SOURCE-FIRST VIEW",
    change: "CHANGE-FIRST VIEW",
  };
  const label = civic$("#sensor-label");
  if (label) label.textContent = labels[sensor] ?? labels.record;
}

async function exportEvidencePacket() {
  const snapshot = currentState();
  const entity = snapshot.selected;
  if (!entity) return announce("SELECT A RECORD FIRST");
  const evidenceIds = collectEvidenceIds(entity);
  const evidence = [];
  if (typeof client !== "undefined" && client?.getEvidence) {
    const results = await Promise.allSettled(evidenceIds.map((id) => client.getEvidence(id)));
    results.forEach((result) => { if (result.status === "fulfilled") evidence.push(result.value); });
  }
  const sources = (entity.source_ids ?? []).map((id) => snapshot.sources?.get?.(id)).filter(Boolean);
  const packet = {
    exported_at: new Date().toISOString(),
    application: "PANOPTICON.FAIL",
    publication_boundary: "sanitized public dataset only",
    entity,
    sources,
    evidence,
  };
  const blob = new Blob([JSON.stringify(packet, null, 2)], { type: "application/json" });
  const link = document.createElement("a");
  link.href = URL.createObjectURL(blob);
  link.download = `${slug(entity.name)}-public-evidence.json`;
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(link.href), 1000);
  announce("PUBLIC EVIDENCE PACK EXPORTED");
}

async function copyCitationPack() {
  const snapshot = currentState();
  const entity = snapshot.selected;
  if (!entity) return announce("SELECT A RECORD FIRST");
  const sources = (entity.source_ids ?? []).map((id) => snapshot.sources?.get?.(id)).filter(Boolean);
  const lines = [
    `PANOPTICON.FAIL PUBLIC RECORD: ${entity.name}`,
    `Record ID: ${entity.id}`,
    `Permanent link: ${location.href}`,
    "",
    "Published sources:",
    ...sources.map((source, index) => `${index + 1}. ${source.title} — ${source.authority ?? "PUBLIC SOURCE"} — ${source.document_date ?? "DATE UNKNOWN"} — SHA-256 ${source.sha256 ?? "UNAVAILABLE"}`),
    "",
    "Every claim should be verified against the linked original public source.",
  ];
  if (typeof copyText === "function") await copyText(lines.join("\n"));
  else await navigator.clipboard?.writeText?.(lines.join("\n"));
  announce("CITATION PACK COPIED");
}

function collectEvidenceIds(entity) {
  return [...new Set([
    ...(entity.attributes ?? []).map((item) => item.evidence_id),
    ...(entity.relationships ?? []).map((item) => item.evidence_id),
    ...(entity.timeline ?? []).map((item) => item.evidence_id),
  ].filter(Boolean))];
}

function toggleWideArea(force) {
  const next = typeof force === "boolean" ? force : document.body.dataset.surface !== "wide";
  document.body.dataset.surface = next ? "wide" : "atlas";
  const button = civic$("#wide-area-view");
  if (button) button.textContent = next ? "RETURN TO CIVIC ATLAS" : "WIDE-AREA LENS";
  atlas?.resize();
  if (typeof globe !== "undefined") globe?.resize?.();
  announce(next ? "WIDE-AREA LENS ENABLED" : "CIVIC ATLAS ENABLED");
}

function announce(message) {
  if (typeof toast === "function") toast(message);
  else {
    const node = civic$("#toast");
    if (node) node.textContent = message;
  }
}

function slug(value) {
  return String(value ?? "record").toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
}
