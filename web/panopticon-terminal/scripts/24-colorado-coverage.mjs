const SVG_NS = "http://www.w3.org/2000/svg";
const KIND_LABELS = {
  state: "State",
  county: "County",
  municipality: "City or town",
  school_district: "Unified school district",
  district_provisional: "District in DOLA discovery layer",
};

export function normalizePlaceQuery(value) {
  return String(value ?? "").trim()
    .replace(/(?:,\s*|\s+)(?:co|colorado)\s*$/i, "")
    .replace(/^county\s+of\s+(.+)$/i, "$1 county")
    .replace(/^(?:city|town)\s+of\s+/i, "")
    .replace(/\s+/g, " ").toLocaleLowerCase();
}

// A typed place can match a municipality or a county even when neither has a
// published decision. The roster is a directory, not evidence of surveillance.
export function findCoveragePlaces(records, query, kind = null) {
  const needle = normalizePlaceQuery(query);
  const priority = { municipality: 0, county: 1, state: 2, school_district: 3, district_provisional: 4 };
  return records.filter((record) => (!kind || record.kind === kind) &&
    (!needle || normalizePlaceQuery(`${record.name} ${record.code}`).includes(needle)))
    .sort((a, b) => (priority[a.kind] ?? 5) - (priority[b.kind] ?? 5) || a.name.localeCompare(b.name));
}

// A geography row cannot create a case. Only a released decision in the
// validated public snapshot can be linked to it.
export function coverageDecisionIndex(records, snapshot) {
  const index = new Map();
  if (snapshot?.mode !== "public") return index;
  const entities = snapshot.entities ?? [];
  const byPublishedId = new Map(records.filter((record) => record.published_entity_id).map((record) => [record.published_entity_id, record.id]));
  const byRef = new Set(records.map((record) => record.id));
  const entityToRecords = new Map();
  for (const entity of entities) {
    if (entity.type !== "JURISDICTION") continue;
    const ids = new Set([
      byPublishedId.get(entity.id),
      byRef.has(entity.jurisdiction_ref) ? entity.jurisdiction_ref : undefined,
    ].filter(Boolean));
    if (ids.size) entityToRecords.set(entity.id, ids);
  }
  for (const entity of entities) {
    if (entity.type !== "DECISION" || !entity.slug) continue;
    const matched = new Set((entity.jurisdiction_ids ?? []).flatMap((id) => [...(entityToRecords.get(id) ?? [])]));
    for (const recordId of matched) {
      if (!index.has(recordId)) index.set(recordId, []);
      index.get(recordId).push(entity);
    }
  }
  return index;
}

export function coverageDecisionsFor(record, snapshot) {
  return record ? coverageDecisionIndex([record], snapshot).get(record.id) ?? [] : [];
}

// The public decision snapshot and geography roster contain no verified search
// audit. Neither a published case nor a directory entry proves a jurisdiction
// was searched. Unknown counts must stay null, including for published places.
export function coverageSearchStatus() {
  return {
    state: "not_established",
    last_searched_at: null,
    lookback_period: null,
    source_count: null,
    document_count: null,
    receipt_count: null,
  };
}

function countyPath(geometry) {
  const polygons = geometry.type === "Polygon" ? [geometry.coordinates] : geometry.coordinates;
  const parts = [];
  for (const polygon of polygons) {
    for (const ring of polygon) {
      if (!ring.length) continue;
      const points = ring.map(([lon, lat]) => [
        ((lon + 109.1) / 7.1 * 700).toFixed(2),
        ((41.1 - lat) / 4.2 * 440).toFixed(2),
      ]);
      parts.push(`M${points.map((point) => point.join(" ")).join("L")}Z`);
    }
  }
  return parts.join("");
}

function element(tag, className, textValue) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (textValue !== undefined) node.textContent = textValue;
  return node;
}

export function installColoradoCoverage({ getSnapshot }) {
  const dialog = document.querySelector("#coverage-dialog");
  const openButton = document.querySelector("#coverage-open");
  if (!dialog || !openButton) return;
  const closeButton = dialog.querySelector("#coverage-close");
  const summary = dialog.querySelector("#coverage-summary");
  const list = dialog.querySelector("#coverage-list");
  const map = dialog.querySelector("#coverage-map");
  const detail = dialog.querySelector("#coverage-detail");
  const search = dialog.querySelector("#coverage-search");
  const resultCount = dialog.querySelector("#coverage-result-count");
  const tabs = [...dialog.querySelectorAll("[data-coverage-kind]")];
  let registry;
  let loadPromise;
  let kind = "municipality";
  let searchAllKinds = false;
  let pendingQuery = null;
  let selectedId = null;
  let indexedEntities = null;
  let indexedMode = null;
  let caseIndex = new Map();
  const overviewMap = document.querySelector("#records-county-map");
  if (overviewMap) fetch("./geography/colorado-jurisdictions.json", {cache:"no-store"})
    .then((response) => { if (!response.ok) throw new Error("County map unavailable"); return response.json(); })
    .then((data) => {
      if (data.schema_version !== 1 || data.counts?.county !== 64 || !Array.isArray(data.jurisdictions)) throw new Error("Invalid county map");
      for (const county of data.jurisdictions.filter((item) => item.kind === "county")) {
        const shape = document.createElementNS(SVG_NS, "path");
        shape.setAttribute("d", countyPath(county.geometry));
        shape.setAttribute("tabindex", "0"); shape.setAttribute("role", "button");
        shape.setAttribute("aria-label", county.name);
        const title = document.createElementNS(SVG_NS, "title"); title.textContent = county.name; shape.append(title);
        const activate = () => open(county.name);
        shape.addEventListener("click", activate);
        shape.addEventListener("keydown", (event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); activate(); } });
        overviewMap.append(shape);
      }
    }).catch(() => { overviewMap.setAttribute("aria-label", "County map unavailable; use place search below"); });

  function ensureCaseIndex() {
    const snapshot = getSnapshot();
    if (snapshot.entities !== indexedEntities || snapshot.mode !== indexedMode) {
      caseIndex = coverageDecisionIndex(registry.jurisdictions, snapshot);
      indexedEntities = snapshot.entities;
      indexedMode = snapshot.mode;
    }
  }

  function casesFor(record) {
    ensureCaseIndex();
    return caseIndex.get(record.id) ?? [];
  }

  function select(record) {
    selectedId = record.id;
    if (kind !== record.kind) {
      kind = record.kind;
    }
    searchAllKinds = false;
    tabs.forEach((tab) => tab.classList.toggle("is-active", tab.dataset.coverageKind === kind));
    renderList();
    renderDetail(record);
    map.querySelectorAll("path").forEach((path) => path.classList.toggle("is-selected", path.dataset.id === record.id));
  }

  function renderMap() {
    map.replaceChildren();
    for (const county of registry.jurisdictions.filter((record) => record.kind === "county")) {
      const path = document.createElementNS(SVG_NS, "path");
      path.setAttribute("d", countyPath(county.geometry));
      path.setAttribute("fill-rule", "evenodd");
      path.setAttribute("tabindex", "0");
      path.setAttribute("role", "button");
      path.setAttribute("aria-label", county.name);
      path.dataset.id = county.id;
      path.addEventListener("click", () => select(county));
      path.addEventListener("keydown", (event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          select(county);
        }
      });
      map.append(path);
    }
  }

  function renderList() {
    if (!registry) return;
    const matches = findCoveragePlaces(registry.jurisdictions, search.value, searchAllKinds ? null : kind);
    resultCount.textContent = `${matches.length.toLocaleString()} matches${matches.length > 100 ? " · showing first 100; narrow the search to find more" : ""}`;
    list.replaceChildren();
    for (const record of matches.slice(0, 100)) {
      const button = element("button", "coverage-result");
      button.type = "button";
      button.classList.toggle("is-selected", record.id === selectedId);
      button.append(element("strong", "", record.name));
      if (searchAllKinds) button.append(element("span", "", KIND_LABELS[record.kind] ?? "Jurisdiction"));
      const decisionCount = casesFor(record).length;
      button.append(element("span", decisionCount ? "coverage-result__published" : "", decisionCount
        ? `${decisionCount} published decision${decisionCount === 1 ? "" : "s"}`
        : "No released decision listed"));
      button.append(element("span", "coverage-result__search", "Search coverage not established"));
      button.addEventListener("click", () => select(record));
      list.append(button);
    }
  }

  function renderDetail(record) {
    detail.replaceChildren();
    const heading = element("h3", "", record.name);
    detail.append(heading);
    detail.append(element("p", "coverage-detail__meta", `${KIND_LABELS[record.kind]} · Official code ${record.code} · ${record.source_locator}`));
    const decisions = casesFor(record);
    if (decisions.length) {
      detail.append(element("p", "coverage-detail__published", `${decisions.length} published decision${decisions.length === 1 ? "" : "s"} linked by a reviewed jurisdiction record.`));
      for (const decision of decisions) {
        const link = element("a", "coverage-decision-link", `Read ${decision.name} and its sources →`);
        link.href = decision.share_url?.includes(`/decisions/${decision.slug}.html`)
          ? `./decisions/${encodeURIComponent(decision.slug)}.html`
          : `./decision.html?decision=${encodeURIComponent(decision.slug)}`;
        detail.append(link);
      }
    } else {
      detail.append(element("p", "coverage-detail__unknown", "No released decision is listed for this place. Relevant records may still exist; explore the preliminary references below."));
    }
    detail.append(element("p", "coverage-detail__unknown", "Search coverage: not established. Last searched: not established. Search period: not established."));
    detail.append(element("p", "coverage-detail__caveat", "Published decisions document individual records. They do not establish that every relevant record in this jurisdiction has been searched. The directory and publication counts do not measure completed searches."));
    const researchLink = element("a", "coverage-decision-link", "Browse preliminary references for this place →");
    researchLink.href = `./research.html?jurisdiction=${encodeURIComponent(record.id)}`;
    detail.append(researchLink);
    if (record.kind === "district_provisional") {
      detail.append(element("p", "coverage-detail__caveat", `DOLA discovery layer · source status code ${record.source_status_code ?? "unknown"} · source update ${record.source_last_update ?? "unknown"}. District identity, current legal status, and boundary need independent checking before a case is attributed.`));
    }
    const source = registry.sources[record.source_id];
    const link = element("a", "coverage-source-link", "Open official geography source (may have changed) ↗");
    link.href = source.url;
    link.target = "_blank";
    link.rel = "noopener noreferrer";
    detail.append(link);
    detail.append(element("p", "coverage-detail__digest", `Archived source bytes SHA-256 ${source.sha256}`));
  }

  function render() {
    const counts = registry.counts;
    ensureCaseIndex();
    const linked = new Set([...caseIndex.values()].flat().map((decision) => decision.id)).size;
    summary.textContent = `${registry.jurisdictions.length.toLocaleString()} geography entries in the directory · 1 state · ${counts.county} counties · ${counts.municipality} cities and towns · ${counts.school_district} school districts · ${counts.district_provisional.toLocaleString()} provisional DOLA district entries · ${linked} linked published decision${linked === 1 ? "" : "s"} · Jurisdiction search coverage not established`;
    renderMap();
    const publishedPlace = registry.jurisdictions.find((record) => record.published_entity_id === "jurisdiction_co_colorado_springs");
    if (pendingQuery !== null) {
      search.value = pendingQuery;
      searchAllKinds = true;
      selectedId = null;
      tabs.forEach((tab) => tab.classList.remove("is-active"));
    } else if (publishedPlace) selectedId = publishedPlace.id;
    renderList();
    if (pendingQuery !== null) {
      detail.replaceChildren(element("p", "coverage-detail__unknown", "Choose a place to see its available records and sources."));
      pendingQuery = null;
      search.focus();
    } else if (publishedPlace) renderDetail(publishedPlace);
  }

  function open(query = null) {
    pendingQuery = query;
    if (!dialog.open) dialog.showModal();
    if (registry) {
      render();
      return;
    }
    if (!loadPromise) {
      loadPromise = fetch("./geography/colorado-jurisdictions.json", { cache: "no-store" })
        .then((response) => {
          if (!response.ok) throw new Error(`Geography unavailable (${response.status})`);
          return response.json();
        })
        .then((data) => {
          if (data.schema_version !== 1 || data.counts?.county !== 64 || !Array.isArray(data.jurisdictions)) {
            throw new Error("Geography registry failed validation");
          }
          registry = data;
          render();
        })
        .catch((error) => {
          summary.textContent = error.message;
          loadPromise = null;
        });
    }
  }

  openButton.addEventListener("click", () => open());
  window.addEventListener("pnull:find-place", (event) => {
    const query = event.detail?.query;
    if (typeof query === "string" && query.trim()) open(query.trim().slice(0, 200));
  });
  closeButton.addEventListener("click", () => dialog.close());
  dialog.addEventListener("click", (event) => { if (event.target === dialog) dialog.close(); });
  tabs.forEach((tab) => tab.addEventListener("click", () => {
    kind = tab.dataset.coverageKind;
    searchAllKinds = false;
    search.value = "";
    tabs.forEach((item) => item.classList.toggle("is-active", item === tab));
    renderList();
  }));
  search.addEventListener("input", renderList);
}
