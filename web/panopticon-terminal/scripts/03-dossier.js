function renderDossier(entity) {
  $("#dossier-empty").hidden = true;
  $("#dossier-record").hidden = false;
  $("#entity-id").textContent = entity.id.toUpperCase();
  $("#entity-type").textContent = entity.type;
  $("#entity-name").textContent = entity.name;
  $("#entity-subtitle").textContent = entity.subtitle ?? "";
  $("#entity-description").textContent = entity.description ?? "No public description available.";
  $("#entity-status").textContent = entity.status ?? "STATUS UNKNOWN";
  $("#entity-location").textContent = entity.geo?.label ?? "NON-SPATIAL RECORD";
  $("#entity-updated").textContent = entity.updated_at ? `UPDATED ${relative(entity.updated_at)}` : "UPDATED UNKNOWN";
  const confidence = averageConfidence(entity);
  $("#confidence-value").textContent = Math.round(confidence * 100);
  $("#confidence-orbit").setAttribute("aria-label", `Evidence confidence ${Math.round(confidence * 100)} percent`);
  renderMetrics(entity);
  renderAttributes(entity);
  renderRelations(entity);
  renderSources(entity);
}

function renderMetrics(entity) {
  const capacity = entity.attributes.find((item) => /power|capacity/i.test(item.label));
  const metrics = [
    ["SOURCES", entity.source_count ?? entity.source_ids.length],
    ["LINKS", entity.relation_count ?? entity.relationships.length],
    capacity ? ["CAPACITY", capacity.value] : ["EVENTS", entity.timeline.length],
  ];
  const grid = $("#metric-grid");
  grid.replaceChildren();
  metrics.forEach(([label, value]) => {
    const div = document.createElement("div");
    div.className = "metric";
    div.innerHTML = `<span>${escapeHtml(label)}</span><strong>${escapeHtml(String(value))}</strong>`;
    grid.append(div);
  });
}

function renderAttributes(entity) {
  const list = $("#attribute-list");
  list.replaceChildren();
  entity.attributes.forEach((item) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "attribute";
    button.innerHTML = `<span>${escapeHtml(item.label)}</span><strong>${escapeHtml(item.value)}</strong><i>↗</i>`;
    if (item.evidence_id) button.addEventListener("click", () => openEvidence(item.evidence_id));
    list.append(button);
  });
}

function renderRelations(entity) {
  const list = $("#relation-list");
  list.replaceChildren();
  $("#relation-count").textContent = `${entity.relationships.length} LINKS`;
  entity.relationships.forEach((item) => {
    const target = state.byId.get(item.target_entity_id);
    const button = document.createElement("button");
    button.type = "button";
    button.className = "relation";
    button.title = "Open related entity. Hold Shift to inspect supporting evidence.";
    button.innerHTML = `<em>${escapeHtml(item.type)}</em><span><b>${escapeHtml(target?.name ?? item.target_entity_id)}</b><small>${escapeHtml(item.label)} · ${item.source_count ?? 1} SOURCE${item.source_count === 1 ? "" : "S"}</small></span><strong>${Math.round(Number(item.confidence ?? 0) * 100)}%</strong>`;
    button.addEventListener("click", (event) => event.shiftKey && item.evidence_id ? openEvidence(item.evidence_id) : selectEntity(item.target_entity_id));
    list.append(button);
  });
}

function renderSources(entity) {
  const list = $("#source-list");
  list.replaceChildren();
  $("#source-count").textContent = `${entity.source_ids.length} SOURCES`;
  entity.source_ids.forEach((id) => {
    const source = state.sources.get(id);
    const button = document.createElement("button");
    button.type = "button";
    button.className = "source-card";
    button.innerHTML = `<b>${escapeHtml(source?.title ?? id)}</b><small>${escapeHtml(source?.authority ?? "PUBLIC SOURCE")} · ${escapeHtml(source?.document_date ?? "DATE UNKNOWN")}</small>`;
    button.addEventListener("click", () => openSource(id));
    list.append(button);
  });
}

function setDossierTab(tab) {
  $$(".dossier-tabs button").forEach((button) => button.classList.toggle("is-active", button.dataset.tab === tab));
  $$(".dossier-pane").forEach((pane) => { pane.hidden = pane.dataset.pane !== tab; });
}
