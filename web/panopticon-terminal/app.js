import { PanopticonClient } from "./api.js";

const client = new PanopticonClient();
const DENSITIES = ["standard", "dense", "terminal"];
const TABS = ["overview", "relations", "events", "sources"];

const state = {
  mode: "connecting",
  status: null,
  activeEntity: null,
  activeTab: "overview",
  entityCache: new Map(),
  sourceCache: new Map(),
  evidenceCache: new Map(),
  searchResults: [],
  selectedSearchIndex: -1,
  searchRequest: 0,
  commandItems: [],
  commandItemsRendered: [],
  selectedCommandIndex: 0,
  density: readDensity(),
};

const elements = {
  app: document.querySelector("#app"),
  terminal: document.querySelector("#main-content"),
  homeView: document.querySelector("#home-view"),
  entityView: document.querySelector("#entity-view"),
  entityWorkspace: document.querySelector("#entity-workspace"),
  homeButton: document.querySelector("#home-button"),
  globalSearch: document.querySelector("#global-search"),
  searchResults: document.querySelector("#search-results"),
  activityList: document.querySelector("#activity-list"),
  activityCount: document.querySelector("#activity-count"),
  systemState: document.querySelector("#system-state"),
  stateLabel: document.querySelector("#state-label"),
  utcClock: document.querySelector("#utc-clock"),
  densityButton: document.querySelector("#density-button"),
  densityLabel: document.querySelector("#density-label"),
  commandButton: document.querySelector("#command-button"),
  commandDialog: document.querySelector("#command-dialog"),
  commandInput: document.querySelector("#command-input"),
  commandResults: document.querySelector("#command-results"),
  evidenceDrawer: document.querySelector("#evidence-drawer"),
  evidenceContent: document.querySelector("#evidence-content"),
  evidenceTitle: document.querySelector("#evidence-title"),
  closeEvidence: document.querySelector("#close-evidence"),
  drawerScrim: document.querySelector("#drawer-scrim"),
  toastRegion: document.querySelector("#toast-region"),
};

init().catch((error) => {
  console.error(error);
  setSystemState("error", "OFFLINE");
  elements.activityList.innerHTML = `<div class="error-panel">${escapeHtml(error.message)}</div>`;
});

async function init() {
  applyDensity(state.density);
  bindEvents();
  updateClock();
  window.setInterval(updateClock, 1000);

  const bootstrap = await client.bootstrap();
  state.mode = bootstrap.mode;
  state.status = bootstrap.status;
  renderStatus(bootstrap.status, bootstrap.mode);
  renderActivity(bootstrap.activity ?? []);

  for (const entity of bootstrap.featured ?? []) {
    if (entity?.id) state.entityCache.set(entity.id, entity);
  }

  const route = readRoute();
  if (route.entityId) await openEntity(route.entityId, { push: false, tab: route.tab });
  else showHome({ push: false });
}

function bindEvents() {
  elements.homeButton.addEventListener("click", () => showHome());
  elements.densityButton.addEventListener("click", cycleDensity);
  elements.commandButton.addEventListener("click", openCommandPalette);
  elements.closeEvidence.addEventListener("click", closeEvidenceDrawer);
  elements.drawerScrim.addEventListener("click", closeEvidenceDrawer);

  for (const button of document.querySelectorAll("[data-query]")) {
    button.addEventListener("click", () => {
      elements.globalSearch.value = button.dataset.query ?? "";
      elements.globalSearch.focus();
      runSearch(elements.globalSearch.value);
    });
  }

  for (const button of document.querySelectorAll(".rail-button")) {
    button.addEventListener("click", () => handleRailAction(button.dataset.action));
  }

  elements.globalSearch.addEventListener("input", () => runSearch(elements.globalSearch.value));
  elements.globalSearch.addEventListener("focus", () => runSearch(elements.globalSearch.value));
  elements.globalSearch.addEventListener("keydown", handleSearchKeydown);

  elements.commandInput.addEventListener("input", renderCommandResults);
  elements.commandInput.addEventListener("keydown", handleCommandKeydown);
  elements.commandDialog.addEventListener("close", () => {
    state.selectedCommandIndex = 0;
    elements.commandInput.value = "";
  });

  document.addEventListener("keydown", handleGlobalKeydown);
  document.addEventListener("pointerdown", (event) => {
    if (!event.target.closest(".search-zone")) closeSearchResults();
  });
  window.addEventListener("popstate", async () => {
    const route = readRoute();
    if (route.entityId) await openEntity(route.entityId, { push: false, tab: route.tab });
    else showHome({ push: false });
  });
}

function renderStatus(status, mode) {
  const counts = status.counts ?? {};
  setText("#metric-entities", formatCount(counts.entities));
  setText("#metric-records", formatCount(counts.records));
  setText("#metric-sources", formatCount(counts.sources));
  setText("#metric-relations", formatCount(counts.relationships));
  setText("#metric-published", relativeTime(status.last_publish));
  setText("#last-ingest", timestamp(status.last_ingest));
  setText("#last-publish", timestamp(status.last_publish));
  setText("#schema-version", status.schema_version);
  setText("#manifest-digest", status.manifest_digest);
  setText("#dataset-version", shortDataset(status.dataset_version));

  if (mode === "demo") {
    setSystemState("demo", "DEMO DATA");
    setText("#api-mode", "MOCK SNAPSHOT");
    setText("#mode-label", "READ ONLY / DEMO");
    return;
  }

  const lastPublish = status.last_publish ? Date.parse(status.last_publish) : NaN;
  const stale = Number.isFinite(lastPublish) && Date.now() - lastPublish > 24 * 60 * 60 * 1000;
  setSystemState(stale || status.state === "stale" ? "stale" : "live", stale ? "STALE" : "LIVE");
  setText("#api-mode", "PUBLIC API V1");
  setText("#mode-label", "READ ONLY");
}

function renderActivity(activity) {
  elements.activityCount.textContent = `${activity.length} EVENTS`;
  if (!activity.length) {
    elements.activityList.innerHTML = '<div class="loading-row">NO PUBLISHED ACTIVITY</div>';
    return;
  }

  elements.activityList.innerHTML = activity
    .map(
      (item) => `
        <button class="activity-row" type="button" data-entity-id="${escapeAttr(item.entity_id ?? "")}">
          <time class="activity-time" datetime="${escapeAttr(item.timestamp ?? "")}">${escapeHtml(shortTime(item.timestamp))}</time>
          <span class="activity-type">${escapeHtml(item.type ?? "EVENT")}</span>
          <span class="activity-summary">${escapeHtml(item.summary ?? "Published record updated")}</span>
          <span class="activity-status">${escapeHtml(item.status ?? "PUBLISHED")}</span>
        </button>`,
    )
    .join("");

  for (const button of elements.activityList.querySelectorAll("[data-entity-id]")) {
    button.addEventListener("click", () => {
      if (button.dataset.entityId) openEntity(button.dataset.entityId);
    });
  }
}

async function runSearch(query) {
  const requestId = ++state.searchRequest;
  try {
    const results = await client.search(query.trim(), { limit: 12 });
    if (requestId !== state.searchRequest) return;
    state.searchResults = results.map(normalizeSearchResult);
    state.selectedSearchIndex = state.searchResults.length ? 0 : -1;
    renderSearchResults(query.trim());
  } catch (error) {
    if (requestId !== state.searchRequest) return;
    state.searchResults = [];
    elements.searchResults.hidden = false;
    elements.searchResults.innerHTML = `<div class="search-empty">SEARCH UNAVAILABLE · ${escapeHtml(error.message)}</div>`;
    elements.globalSearch.setAttribute("aria-expanded", "true");
  }
}

function renderSearchResults(query) {
  elements.searchResults.hidden = false;
  elements.globalSearch.setAttribute("aria-expanded", "true");

  if (!state.searchResults.length) {
    elements.searchResults.innerHTML = `<div class="search-empty">NO PUBLIC RECORDS MATCH “${escapeHtml(query)}”</div>`;
    return;
  }

  elements.searchResults.innerHTML = state.searchResults
    .map(
      (result, index) => `
        <button class="search-result ${index === state.selectedSearchIndex ? "is-selected" : ""}"
          type="button" role="option" aria-selected="${index === state.selectedSearchIndex}"
          data-result-index="${index}">
          <span class="result-type">${escapeHtml(result.type)}</span>
          <span class="result-name">
            <strong>${highlight(result.name, query)}</strong>
            <span>${escapeHtml(result.subtitle)}</span>
          </span>
          <span class="result-meta">${result.kind === "source" ? "SOURCE RECORD" : `${formatCount(result.source_count)} SOURCES`}</span>
        </button>`,
    )
    .join("");

  for (const button of elements.searchResults.querySelectorAll("[data-result-index]")) {
    button.addEventListener("mouseenter", () => {
      state.selectedSearchIndex = Number(button.dataset.resultIndex);
      updateSearchSelection();
    });
    button.addEventListener("click", () => activateSearchResult(Number(button.dataset.resultIndex)));
  }
}

function handleSearchKeydown(event) {
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    if (!state.searchResults.length) return;
    const direction = event.key === "ArrowDown" ? 1 : -1;
    state.selectedSearchIndex =
      (state.selectedSearchIndex + direction + state.searchResults.length) % state.searchResults.length;
    updateSearchSelection();
  } else if (event.key === "Enter" && state.selectedSearchIndex >= 0) {
    event.preventDefault();
    activateSearchResult(state.selectedSearchIndex);
  } else if (event.key === "Escape") {
    closeSearchResults();
    elements.globalSearch.blur();
  }
}

function updateSearchSelection() {
  elements.searchResults.querySelectorAll("[data-result-index]").forEach((button, index) => {
    const selected = index === state.selectedSearchIndex;
    button.classList.toggle("is-selected", selected);
    button.setAttribute("aria-selected", String(selected));
    if (selected) button.scrollIntoView({ block: "nearest" });
  });
}

function activateSearchResult(index) {
  const result = state.searchResults[index];
  if (!result) return;
  closeSearchResults();
  elements.globalSearch.value = "";
  if (result.kind === "source") openSourceDrawer(result.id);
  else openEntity(result.id);
}

function closeSearchResults() {
  elements.searchResults.hidden = true;
  elements.globalSearch.setAttribute("aria-expanded", "false");
  state.selectedSearchIndex = -1;
}

async function openEntity(id, { push = true, tab = "overview" } = {}) {
  closeSearchResults();
  closeEvidenceDrawer();
  elements.homeView.hidden = true;
  elements.entityView.hidden = false;
  elements.entityWorkspace.innerHTML = '<div class="loading-row">LOADING PUBLIC ENTITY WORKSPACE…</div>';
  elements.terminal.scrollTo({ top: 0 });

  try {
    let entity = state.entityCache.get(id);
    if (!entity?.relationships) {
      entity = await client.getEntity(id);
      state.entityCache.set(id, entity);
    }

    state.activeEntity = entity;
    state.activeTab = TABS.includes(tab) ? tab : "overview";

    const targetIds = (entity.relationships ?? []).map((relationship) => relationship.target_entity_id);
    const missingTargetIds = targetIds.filter((targetId) => !state.entityCache.has(targetId));
    for (const target of await client.getEntitiesByIds(missingTargetIds)) state.entityCache.set(target.id, target);

    const sourceResults = await Promise.allSettled(
      (entity.source_ids ?? []).map(async (sourceId) => {
        if (state.sourceCache.has(sourceId)) return state.sourceCache.get(sourceId);
        const source = await client.getSource(sourceId);
        state.sourceCache.set(sourceId, source);
        return source;
      }),
    );
    const sources = sourceResults.filter((result) => result.status === "fulfilled").map((result) => result.value);

    renderEntity(entity, sources);
    setRailForTab(state.activeTab);
    if (push) updateRoute(entity.id, state.activeTab);
  } catch (error) {
    console.error(error);
    elements.entityWorkspace.innerHTML = `<div class="error-panel">PUBLIC ENTITY COULD NOT BE OPENED<br>${escapeHtml(error.message)}</div>`;
  }
}

function renderEntity(entity, sources) {
  elements.entityWorkspace.innerHTML = `
    <article class="entity-workspace">
      <header class="entity-header">
        <div class="entity-title-row">
          <div>
            <p class="entity-path">ENTITY / ${escapeHtml(entity.type ?? "UNKNOWN")}</p>
            <h1 class="entity-title">${escapeHtml(entity.name)}</h1>
            <p class="entity-subtitle">${escapeHtml(entity.subtitle ?? "Public entity record")}</p>
          </div>
          <div class="entity-id-block"><span>PUBLIC RECORD ID</span><code>${escapeHtml(entity.id)}</code></div>
        </div>
        <nav class="entity-tabs" aria-label="Entity workspace views">
          ${tabButton("overview", "OVERVIEW")}${tabButton("relations", "RELATIONS")}
          ${tabButton("events", "EVENTS")}${tabButton("sources", "SOURCES")}
        </nav>
      </header>
      <div class="entity-body" id="entity-tab-content"></div>
    </article>`;

  for (const button of elements.entityWorkspace.querySelectorAll("[data-tab]")) {
    button.addEventListener("click", () => setEntityTab(button.dataset.tab, sources));
  }
  renderEntityTab(entity, sources);
}

function tabButton(id, label) {
  return `<button class="entity-tab ${state.activeTab === id ? "is-active" : ""}" type="button" data-tab="${id}">${label}</button>`;
}

function setEntityTab(tab, sources = null) {
  if (!state.activeEntity || !TABS.includes(tab)) return;
  state.activeTab = tab;
  const resolvedSources =
    sources ?? (state.activeEntity.source_ids ?? []).map((id) => state.sourceCache.get(id)).filter(Boolean);
  renderEntityTab(state.activeEntity, resolvedSources);
  for (const button of elements.entityWorkspace.querySelectorAll("[data-tab]")) {
    button.classList.toggle("is-active", button.dataset.tab === tab);
  }
  setRailForTab(tab);
  updateRoute(state.activeEntity.id, tab, { replace: true });
}

function renderEntityTab(entity, sources) {
  const content = document.querySelector("#entity-tab-content");
  if (!content) return;

  if (state.activeTab === "relations") {
    content.innerHTML = `<section class="entity-panel"><header><h2>DOCUMENTED RELATIONSHIPS</h2><span>${(entity.relationships ?? []).length} PUBLIC EDGES</span></header>${relationList(entity)}</section>`;
  } else if (state.activeTab === "events") {
    content.innerHTML = `<section class="entity-panel"><header><h2>PUBLIC TIMELINE</h2><span>${(entity.timeline ?? []).length} EVENTS</span></header>${timelineList(entity)}</section>`;
  } else if (state.activeTab === "sources") {
    content.innerHTML = `<section class="entity-panel"><header><h2>PUBLISHED SOURCES</h2><span>${sources.length} LOADED / ${formatCount(entity.source_count)} TOTAL</span></header>${sourceList(sources)}</section>`;
  } else {
    content.innerHTML = `
      <div class="entity-overview-grid">
        <div class="entity-main-column">
          <section class="entity-panel graph-panel">
            <header><h2>RELATION GRAPH</h2><span>DEPTH 1 · CLICK EDGE FOR EVIDENCE</span></header>
            <div class="graph-stage" id="graph-stage" aria-label="Relationship graph"></div>
            <div class="graph-legend"><span>FOCUS ENTITY</span><span>DOCUMENTED RELATION</span><span>CLICK TO RE-CENTER</span></div>
          </section>
          <section class="entity-panel"><header><h2>RECENT EVENTS</h2><span>${(entity.timeline ?? []).length} OBSERVED</span></header>${timelineList(entity, 4)}</section>
        </div>
        <aside class="entity-side-column">
          <section class="entity-panel"><header><h2>ENTITY FACTS</h2><span>PUBLIC FIELDS</span></header>${factList(entity)}</section>
          <section class="entity-panel"><header><h2>SUMMARY</h2><span>NON-INFERENTIAL</span></header><p class="entity-description">${escapeHtml(entity.description ?? "No public summary is available.")}</p></section>
          <section class="entity-panel"><header><h2>CLASSIFICATION</h2><span>${(entity.tags ?? []).length} TAGS</span></header><div class="tag-list">${(entity.tags ?? []).map((tag) => `<span class="tag">${escapeHtml(tag)}</span>`).join("")}</div></section>
          <section class="entity-panel"><header><h2>RELATIONSHIPS</h2><span>${(entity.relationships ?? []).length} EDGES</span></header>${relationList(entity, 4)}</section>
        </aside>
      </div>`;
    renderGraph(entity);
  }

  bindWorkspaceActions();
}

function factList(entity) {
  const facts = [
    ...(entity.attributes ?? []),
    { label: "UPDATED", value: timestamp(entity.updated_at) },
    { label: "SOURCE COUNT", value: formatCount(entity.source_count) },
  ];
  return `<dl class="fact-list">${facts
    .map(
      (fact) => `<div><dt>${escapeHtml(fact.label)}</dt><dd class="fact-value-row"><span>${escapeHtml(String(fact.value ?? "Unknown"))}</span>${fact.evidence_id ? `<button class="evidence-link" type="button" data-evidence-id="${escapeAttr(fact.evidence_id)}">EVIDENCE</button>` : ""}</dd></div>`,
    )
    .join("")}</dl>`;
}

function relationList(entity, limit = Infinity) {
  const relationships = (entity.relationships ?? []).slice(0, limit);
  if (!relationships.length) return '<div class="workspace-empty">NO PUBLISHED RELATIONSHIPS</div>';
  return `<div class="relation-list">${relationships
    .map((relationship) => {
      const target = state.entityCache.get(relationship.target_entity_id);
      return `<div class="relation-row">
        <span class="relation-kind">${escapeHtml(relationship.type ?? relationship.label ?? "RELATED_TO")}</span>
        <button class="relation-target" type="button" data-entity-id="${escapeAttr(relationship.target_entity_id)}">${escapeHtml(target?.name ?? relationship.target_name ?? relationship.target_entity_id)}</button>
        <span class="relation-meta">${Math.round(Number(relationship.confidence ?? 1) * 100)}% · ${formatCount(relationship.source_count ?? 0)} SRC</span>
        ${relationship.evidence_id ? `<button class="evidence-link" type="button" data-evidence-id="${escapeAttr(relationship.evidence_id)}">EVIDENCE</button>` : ""}
      </div>`;
    })
    .join("")}</div>`;
}

function timelineList(entity, limit = Infinity) {
  const timeline = (entity.timeline ?? []).slice(0, limit);
  if (!timeline.length) return '<div class="workspace-empty">NO PUBLISHED EVENTS</div>';
  return `<div class="timeline-list">${timeline
    .map(
      (event) => `<div class="timeline-row">
        <time class="timeline-date" datetime="${escapeAttr(event.date ?? "")}">${escapeHtml(event.date ?? "UNKNOWN")}</time>
        <span class="timeline-type">${escapeHtml(event.type ?? "EVENT")}</span>
        <span class="timeline-title">${escapeHtml(event.title ?? "Published event")}</span>
        ${event.evidence_id ? `<button class="evidence-link" type="button" data-evidence-id="${escapeAttr(event.evidence_id)}">EVIDENCE</button>` : ""}
      </div>`,
    )
    .join("")}</div>`;
}

function sourceList(sources) {
  if (!sources.length) return '<div class="workspace-empty">NO PUBLIC SOURCES LOADED</div>';
  return `<div class="source-list">${sources
    .map(
      (source) => `<div class="source-row">
        <span class="source-authority">${escapeHtml(source.authority ?? "UNKNOWN")}</span>
        <div class="source-title"><strong>${escapeHtml(source.title ?? source.id)}</strong><span>${escapeHtml(shortDigest(source.sha256))}</span></div>
        <time class="source-date" datetime="${escapeAttr(source.retrieved_at ?? "")}">${escapeHtml(timestamp(source.retrieved_at))}</time>
        <button class="evidence-link" type="button" data-source-id="${escapeAttr(source.id)}">OPEN</button>
      </div>`,
    )
    .join("")}</div>`;
}

function bindWorkspaceActions() {
  for (const button of elements.entityWorkspace.querySelectorAll("[data-entity-id]")) {
    button.addEventListener("click", () => openEntity(button.dataset.entityId));
  }
  for (const button of elements.entityWorkspace.querySelectorAll("[data-evidence-id]")) {
    button.addEventListener("click", () => openEvidenceDrawer(button.dataset.evidenceId));
  }
  for (const button of elements.entityWorkspace.querySelectorAll("[data-source-id]")) {
    button.addEventListener("click", () => openSourceDrawer(button.dataset.sourceId));
  }
}

function renderGraph(entity) {
  const stage = document.querySelector("#graph-stage");
  if (!stage) return;
  const relationships = (entity.relationships ?? []).slice(0, 8);
  if (!relationships.length) {
    stage.innerHTML = '<div class="workspace-empty">NO PUBLISHED RELATIONSHIPS</div>';
    return;
  }

  const svg = createSvg("svg", {
    viewBox: "0 0 800 395",
    role: "img",
    "aria-label": `Relationship graph centered on ${entity.name}`,
  });
  const center = { x: 400, y: 197 };
  const positions = relationships.map((_, index) => {
    const angle = -Math.PI / 2 + (index * Math.PI * 2) / relationships.length;
    return { x: center.x + Math.cos(angle) * 265, y: center.y + Math.sin(angle) * 138 };
  });

  relationships.forEach((relationship, index) => {
    const targetPosition = positions[index];
    const target = state.entityCache.get(relationship.target_entity_id);
    const edgeGroup = createSvg("g", { class: "graph-edge-group" });
    const line = createSvg("line", {
      class: "graph-edge",
      x1: center.x,
      y1: center.y,
      x2: targetPosition.x,
      y2: targetPosition.y,
    });
    const hitLine = createSvg("line", {
      x1: center.x,
      y1: center.y,
      x2: targetPosition.x,
      y2: targetPosition.y,
      stroke: "transparent",
      "stroke-width": "16",
      cursor: relationship.evidence_id ? "pointer" : "default",
    });
    if (relationship.evidence_id) {
      hitLine.addEventListener("click", () => openEvidenceDrawer(relationship.evidence_id));
      hitLine.addEventListener("mouseenter", () => line.classList.add("is-active"));
      hitLine.addEventListener("mouseleave", () => line.classList.remove("is-active"));
    }
    edgeGroup.append(line, hitLine);

    const midX = (center.x + targetPosition.x) / 2;
    const midY = (center.y + targetPosition.y) / 2;
    const label = String(relationship.label ?? relationship.type ?? "related to").toUpperCase();
    const labelWidth = Math.min(124, Math.max(54, label.length * 5.8 + 14));
    edgeGroup.append(createSvg("rect", { class: "graph-edge-label-bg", x: midX - labelWidth / 2, y: midY - 8, width: labelWidth, height: 16 }));
    const labelText = createSvg("text", { class: "graph-edge-label", x: midX, y: midY + 3 });
    labelText.textContent = truncate(label, 19);
    edgeGroup.append(labelText);
    svg.append(edgeGroup);

    svg.append(
      graphNode({
        entity: target ?? { id: relationship.target_entity_id, type: "ENTITY", name: relationship.target_name ?? relationship.target_entity_id },
        x: targetPosition.x,
        y: targetPosition.y,
        center: false,
      }),
    );
  });

  svg.append(graphNode({ entity, x: center.x, y: center.y, center: true }));
  stage.replaceChildren(svg);
}

function graphNode({ entity, x, y, center }) {
  const width = center ? 174 : 154;
  const height = center ? 60 : 54;
  const group = createSvg("g", {
    class: `graph-node ${center ? "is-center" : ""}`,
    transform: `translate(${x - width / 2} ${y - height / 2})`,
    tabindex: "0",
    role: "button",
    "aria-label": `${center ? "Current" : "Open"} entity ${entity.name}`,
  });
  group.append(createSvg("rect", { width, height }));
  const type = createSvg("text", { class: "node-type", x: 11, y: 18 });
  type.textContent = String(entity.type ?? "ENTITY").toUpperCase();
  const name = createSvg("text", { class: "node-name", x: 11, y: 39 });
  name.textContent = truncate(entity.name ?? entity.id, center ? 25 : 21);
  group.append(type, name);

  if (!center) {
    const activate = () => openEntity(entity.id);
    group.addEventListener("click", activate);
    group.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        activate();
      }
    });
  }
  return group;
}

async function openEvidenceDrawer(evidenceId) {
  if (!evidenceId) return;
  elements.evidenceTitle.textContent = "Claim record";
  elements.evidenceContent.innerHTML = '<div class="loading-row">LOADING PUBLIC EVIDENCE…</div>';
  openDrawerShell();

  try {
    let evidence = state.evidenceCache.get(evidenceId);
    if (!evidence) {
      evidence = await client.getEvidence(evidenceId);
      state.evidenceCache.set(evidenceId, evidence);
    }
    let source = state.sourceCache.get(evidence.source_id);
    if (!source) {
      source = await client.getSource(evidence.source_id);
      state.sourceCache.set(source.id, source);
    }
    elements.evidenceContent.innerHTML = evidenceMarkup(evidence, source);
    bindDrawerActions();
  } catch (error) {
    elements.evidenceContent.innerHTML = `<div class="error-panel">${escapeHtml(error.message)}</div>`;
  }
}

async function openSourceDrawer(sourceId) {
  if (!sourceId) return;
  elements.evidenceTitle.textContent = "Source record";
  elements.evidenceContent.innerHTML = '<div class="loading-row">LOADING PUBLIC SOURCE…</div>';
  openDrawerShell();

  try {
    let source = state.sourceCache.get(sourceId);
    if (!source) {
      source = await client.getSource(sourceId);
      state.sourceCache.set(source.id, source);
    }
    elements.evidenceContent.innerHTML = sourceMarkup(source);
    bindDrawerActions();
  } catch (error) {
    elements.evidenceContent.innerHTML = `<div class="error-panel">${escapeHtml(error.message)}</div>`;
  }
}

function evidenceMarkup(evidence, source) {
  return `
    <div class="evidence-status"><span class="review-badge">${escapeHtml(evidence.review_state ?? "APPROVED")}</span><p>Approval is bound to the exact published evidence digest and public fields.</p></div>
    <section class="claim-block"><span class="block-label">CLAIM</span><p>${escapeHtml(evidence.claim ?? "No public claim text available.")}</p></section>
    <section class="quote-block"><span class="block-label">EXACT SOURCE EXCERPT</span><blockquote>${escapeHtml(evidence.quote ?? "No public excerpt available.")}</blockquote></section>
    <dl class="evidence-metadata">
      ${metadata("AUTHORITY", evidence.authority ?? source.authority)}${metadata("LOCATOR", evidence.locator)}
      ${metadata("RETRIEVED", timestamp(evidence.retrieved_at))}${metadata("PAGE", evidence.page ?? "—")}
      ${metadata("SOURCE DIGEST", evidence.sha256)}${metadata("REVIEW BINDING", evidence.review_bound_digest ?? "—")}
      ${metadata("EVIDENCE ID", evidence.id)}${metadata("SOURCE ID", source.id)}
    </dl>
    <section class="source-block"><span class="block-label">SOURCE</span><p>${escapeHtml(source.title ?? source.id)}</p></section>
    ${drawerActions(source, evidence.sha256)}`;
}

function sourceMarkup(source) {
  return `
    <div class="evidence-status"><span class="review-badge">PUBLIC</span><p>This source record exists in the sanitized, read-only publication dataset.</p></div>
    <section class="claim-block"><span class="block-label">SOURCE TITLE</span><p>${escapeHtml(source.title ?? source.id)}</p></section>
    <section class="source-block"><span class="block-label">DESCRIPTION</span><p>${escapeHtml(source.description ?? "No public source description available.")}</p></section>
    <dl class="evidence-metadata">
      ${metadata("AUTHORITY", source.authority)}${metadata("SOURCE TYPE", source.source_type)}
      ${metadata("DOCUMENT DATE", source.document_date ?? "—")}${metadata("RETRIEVED", timestamp(source.retrieved_at))}
      ${metadata("SOURCE DIGEST", source.sha256)}${metadata("SOURCE ID", source.id)}
    </dl>
    ${drawerActions(source, source.sha256)}`;
}

function metadata(label, value) {
  return `<div><dt>${escapeHtml(label)}</dt><dd>${escapeHtml(String(value ?? "—"))}</dd></div>`;
}

function drawerActions(source, digest) {
  const safeUrl = publicHttpUrl(source.canonical_url);
  const sourceAction =
    source.demo || !safeUrl
      ? '<button class="secondary-action" type="button" data-demo-source>DEMO SOURCE</button>'
      : `<a class="primary-action" href="${escapeAttr(safeUrl)}" target="_blank" rel="noopener noreferrer">VIEW ORIGINAL</a>`;
  return `<div class="evidence-actions">${sourceAction}<button class="secondary-action" type="button" data-copy-value="${escapeAttr(digest ?? "")}">COPY DIGEST</button></div>`;
}

function bindDrawerActions() {
  for (const button of elements.evidenceContent.querySelectorAll("[data-copy-value]")) {
    button.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(button.dataset.copyValue ?? "");
        showToast("DIGEST COPIED");
      } catch {
        showToast("CLIPBOARD UNAVAILABLE");
      }
    });
  }
  for (const button of elements.evidenceContent.querySelectorAll("[data-demo-source]")) {
    button.addEventListener("click", () => showToast("SYNTHETIC SOURCE · NO EXTERNAL DOCUMENT"));
  }
}

function openDrawerShell() {
  elements.evidenceDrawer.classList.add("is-open");
  elements.evidenceDrawer.setAttribute("aria-hidden", "false");
  elements.drawerScrim.hidden = false;
}

function closeEvidenceDrawer() {
  elements.evidenceDrawer.classList.remove("is-open");
  elements.evidenceDrawer.setAttribute("aria-hidden", "true");
  elements.drawerScrim.hidden = true;
}

function showHome({ push = true } = {}) {
  closeEvidenceDrawer();
  closeSearchResults();
  state.activeEntity = null;
  state.activeTab = "overview";
  elements.entityView.hidden = true;
  elements.homeView.hidden = false;
  elements.terminal.scrollTo({ top: 0 });
  setRailActive("home");
  if (push) history.pushState({}, "", location.pathname);
}

function handleRailAction(action) {
  if (action === "home") showHome();
  else if (action === "command") openCommandPalette();
  else if (!state.activeEntity) {
    showToast("OPEN AN ENTITY WORKSPACE FIRST");
    elements.globalSearch.focus();
  } else if (action === "graph") setEntityTab("overview");
  else if (action === "timeline") setEntityTab("events");
  else if (action === "sources") setEntityTab("sources");
}

function openCommandPalette() {
  state.commandItems = buildCommands();
  state.selectedCommandIndex = 0;
  elements.commandInput.value = "";
  renderCommandResults();
  elements.commandDialog.showModal();
  queueMicrotask(() => elements.commandInput.focus());
}

function buildCommands() {
  const commands = [
    { id: "home", icon: "/", title: "Open public search", description: "Return to the terminal home screen", key: "/", action: () => showHome() },
    { id: "focus-search", icon: "Q", title: "Focus global search", description: "Search entities, sources, facilities, contracts, and events", key: "/", action: () => { showHome(); queueMicrotask(() => elements.globalSearch.focus()); } },
    { id: "density", icon: "D", title: "Cycle information density", description: `Current mode: ${state.density.toUpperCase()}`, key: "D", action: cycleDensity },
    { id: "copy-link", icon: "↗", title: "Copy permanent workspace link", description: "Copy the current public URL", key: "", action: copyCurrentUrl },
  ];

  if (state.activeEntity) {
    commands.splice(
      2,
      0,
      { id: "graph", icon: "G", title: "Open relation graph", description: `Center graph on ${state.activeEntity.name}`, key: "G", action: () => setEntityTab("overview") },
      { id: "events", icon: "T", title: "Open public timeline", description: `Show events for ${state.activeEntity.name}`, key: "T", action: () => setEntityTab("events") },
      { id: "sources", icon: "S", title: "Open source list", description: `Show sources for ${state.activeEntity.name}`, key: "S", action: () => setEntityTab("sources") },
    );
  }
  return commands;
}

function renderCommandResults() {
  const query = elements.commandInput.value.trim().toLocaleLowerCase();
  const items = state.commandItems.filter((item) =>
    [item.title, item.description, item.id].join(" ").toLocaleLowerCase().includes(query),
  );
  if (query) {
    items.push({
      id: "query",
      icon: "Q",
      title: `Search for “${elements.commandInput.value.trim()}”`,
      description: "Run as a public intelligence query",
      key: "ENTER",
      action: () => {
        const value = elements.commandInput.value.trim();
        showHome();
        elements.globalSearch.value = value;
        elements.globalSearch.focus();
        runSearch(value);
      },
    });
  }

  state.commandItemsRendered = items;
  state.selectedCommandIndex = Math.min(state.selectedCommandIndex, Math.max(0, items.length - 1));
  elements.commandResults.innerHTML = `<div class="command-section-label">AVAILABLE ACTIONS</div>${items
    .map(
      (item, index) => `<button class="command-item ${index === state.selectedCommandIndex ? "is-selected" : ""}" type="button" data-command-index="${index}"><span class="command-icon">${escapeHtml(item.icon)}</span><span class="command-copy"><strong>${escapeHtml(item.title)}</strong><span>${escapeHtml(item.description)}</span></span><span class="command-key">${escapeHtml(item.key ?? "")}</span></button>`,
    )
    .join("")}`;

  for (const button of elements.commandResults.querySelectorAll("[data-command-index]")) {
    button.addEventListener("mouseenter", () => {
      state.selectedCommandIndex = Number(button.dataset.commandIndex);
      updateCommandSelection();
    });
    button.addEventListener("click", () => activateCommand(Number(button.dataset.commandIndex)));
  }
}

function handleCommandKeydown(event) {
  const items = state.commandItemsRendered;
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    if (!items.length) return;
    const direction = event.key === "ArrowDown" ? 1 : -1;
    state.selectedCommandIndex = (state.selectedCommandIndex + direction + items.length) % items.length;
    updateCommandSelection();
  } else if (event.key === "Enter") {
    event.preventDefault();
    activateCommand(state.selectedCommandIndex);
  }
}

function updateCommandSelection() {
  elements.commandResults.querySelectorAll("[data-command-index]").forEach((button, index) => {
    button.classList.toggle("is-selected", index === state.selectedCommandIndex);
    if (index === state.selectedCommandIndex) button.scrollIntoView({ block: "nearest" });
  });
}

function activateCommand(index) {
  const item = state.commandItemsRendered[index];
  if (!item) return;
  elements.commandDialog.close();
  item.action();
}

function handleGlobalKeydown(event) {
  const target = event.target;
  const typing = target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target?.isContentEditable;

  if ((event.metaKey || event.ctrlKey) && event.key.toLocaleLowerCase() === "k") {
    event.preventDefault();
    openCommandPalette();
    return;
  }
  if (event.key === "Escape" && elements.evidenceDrawer.classList.contains("is-open")) {
    event.preventDefault();
    closeEvidenceDrawer();
    return;
  }
  if (typing || event.metaKey || event.ctrlKey || event.altKey) return;

  const key = event.key.toLocaleLowerCase();
  if (key === "/") {
    event.preventDefault();
    showHome();
    elements.globalSearch.focus();
  } else if (key === "g" && state.activeEntity) setEntityTab("overview");
  else if (key === "t" && state.activeEntity) setEntityTab("events");
  else if (key === "s" && state.activeEntity) setEntityTab("sources");
  else if (key === "[") navigateEntity(-1);
  else if (key === "]") navigateEntity(1);
}

function navigateEntity(direction) {
  const entities = [...state.entityCache.values()].filter((item) => item?.id && item?.name);
  if (!state.activeEntity || entities.length < 2) return;
  const index = entities.findIndex((entity) => entity.id === state.activeEntity.id);
  openEntity(entities[(index + direction + entities.length) % entities.length].id);
}

function cycleDensity() {
  state.density = DENSITIES[(DENSITIES.indexOf(state.density) + 1) % DENSITIES.length];
  localStorage.setItem("pnull-density", state.density);
  applyDensity(state.density);
  showToast(`DENSITY ${state.density.toUpperCase()}`);
  if (state.activeEntity && state.activeTab === "overview") renderGraph(state.activeEntity);
}

function applyDensity(density) {
  elements.app.dataset.density = density;
  elements.densityLabel.textContent = density.toUpperCase();
}

function readDensity() {
  const value = localStorage.getItem("pnull-density");
  return DENSITIES.includes(value) ? value : "standard";
}

async function copyCurrentUrl() {
  try {
    await navigator.clipboard.writeText(location.href);
    showToast("PERMANENT LINK COPIED");
  } catch {
    showToast("CLIPBOARD UNAVAILABLE");
  }
}

function setRailForTab(tab) {
  if (tab === "events") setRailActive("timeline");
  else if (tab === "sources") setRailActive("sources");
  else setRailActive("graph");
}

function setRailActive(action) {
  for (const button of document.querySelectorAll(".rail-button")) {
    button.classList.toggle("is-active", button.dataset.action === action);
  }
}

function updateRoute(entityId, tab, { replace = false } = {}) {
  const url = new URL(location.href);
  url.search = "";
  url.searchParams.set("entity", entityId);
  if (tab && tab !== "overview") url.searchParams.set("tab", tab);
  history[replace ? "replaceState" : "pushState"]({}, "", `${url.pathname}${url.search}`);
}

function readRoute() {
  const params = new URLSearchParams(location.search);
  return { entityId: params.get("entity"), tab: params.get("tab") ?? "overview" };
}

function setSystemState(value, label) {
  elements.systemState.dataset.state = value;
  elements.stateLabel.textContent = label;
}

function updateClock() {
  const value = new Date().toISOString();
  elements.utcClock.dateTime = value;
  elements.utcClock.textContent = `${value.slice(11, 19)} UTC`;
}

function showToast(message) {
  const toast = document.createElement("div");
  toast.className = "toast";
  toast.textContent = message;
  elements.toastRegion.append(toast);
  window.setTimeout(() => toast.remove(), 2400);
}

function normalizeSearchResult(result) {
  const kind = result.kind ?? (String(result.type).toUpperCase() === "SOURCE" ? "source" : "entity");
  return {
    kind,
    id: result.id,
    type: String(result.type ?? (kind === "source" ? "SOURCE" : "ENTITY")).toUpperCase(),
    name: result.name ?? result.title ?? result.id,
    subtitle: result.subtitle ?? result.description ?? "Public record",
    source_count: Number(result.source_count ?? result.sources ?? 0),
  };
}

function setText(selector, value) {
  const element = document.querySelector(selector);
  if (element) element.textContent = value ?? "—";
}

function formatCount(value) {
  const number = Number(value ?? 0);
  return Number.isFinite(number) ? new Intl.NumberFormat("en-US").format(number) : "—";
}

function timestamp(value) {
  if (!value) return "—";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return String(value);
  return `${date.toISOString().slice(0, 16).replace("T", " ")} UTC`;
}

function relativeTime(value) {
  if (!value) return "—";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "—";
  const seconds = Math.max(0, Math.floor((Date.now() - date.getTime()) / 1000));
  if (seconds < 60) return `${seconds}s ago`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  return `${Math.floor(seconds / 86400)}d ago`;
}

function shortTime(value) {
  if (!value) return "--:--:--";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "--:--:--" : date.toISOString().slice(11, 19);
}

function shortDataset(value) {
  if (!value) return "DATASET —";
  return `DATASET ${truncate(String(value).replace(/^demo-/, "D/").replace(/^\d{4}-/, ""), 22)}`;
}

function shortDigest(value) {
  if (!value) return "NO DIGEST";
  const digest = String(value).replace(/^sha256:/, "");
  return `SHA256 ${digest.slice(0, 12)}…${digest.slice(-6)}`;
}

function truncate(value, length) {
  const text = String(value ?? "");
  return text.length > length ? `${text.slice(0, Math.max(1, length - 1))}…` : text;
}

function highlight(value, query) {
  const text = String(value ?? "");
  const term = query.replace(/\b(?:type|source):[^\s]+/gi, "").trim().split(/\s+/).find(Boolean);
  if (!term) return escapeHtml(text);
  const index = text.toLocaleLowerCase().indexOf(term.toLocaleLowerCase());
  if (index < 0) return escapeHtml(text);
  return `${escapeHtml(text.slice(0, index))}<mark>${escapeHtml(text.slice(index, index + term.length))}</mark>${escapeHtml(text.slice(index + term.length))}`;
}

function publicHttpUrl(value) {
  if (!value) return null;
  try {
    const url = new URL(value, location.origin);
    return ["http:", "https:"].includes(url.protocol) ? url.href : null;
  } catch {
    return null;
  }
}

function createSvg(name, attributes = {}) {
  const element = document.createElementNS("http://www.w3.org/2000/svg", name);
  for (const [key, value] of Object.entries(attributes)) element.setAttribute(key, String(value));
  return element;
}

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

function escapeAttr(value) {
  return escapeHtml(value).replaceAll("`", "&#096;");
}
