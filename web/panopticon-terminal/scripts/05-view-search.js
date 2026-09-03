function setView(view) {
  state.view = view;
  $$(".view-tab").forEach((button) => button.classList.toggle("is-active", button.dataset.view === view));
  globe.setView(view);
  if (view === "network") focusNetwork();
  else if (view === "records") openSearch("source:official");
  else globe.reset();
}
function focusNetwork() {
  state.view = "network";
  $$(".view-tab").forEach((button) => button.classList.toggle("is-active", button.dataset.view === "network"));
  globe.setView("network");
  globe.focusNetwork(state.selected?.id);
}
function resetGlobal() {
  state.view = "global";
  $$(".view-tab").forEach((button) => button.classList.toggle("is-active", button.dataset.view === "global"));
  globe.setView("global"); globe.reset();
}
function toggleLayer(button) {
  const layer = button.dataset.layer;
  state.activeLayers.has(layer) ? state.activeLayers.delete(layer) : state.activeLayers.add(layer);
  button.classList.toggle("is-active", state.activeLayers.has(layer));
  globe.setLayers(state.activeLayers); updateVisibleCounts();
}
function setSensor(sensor) {
  state.sensor = sensor;
  document.body.dataset.sensor = sensor;
  $$(".sensor-tab").forEach((button) => button.classList.toggle("is-active", button.dataset.sensor === sensor));
  $("#sensor-label").textContent = sensor === "night" ? "LOW-LIGHT PUBLIC VIEW" : sensor === "change" ? "CHANGE INTENSITY" : "PUBLIC RECORD";
  globe.setSensor(sensor);
}
function cycleDensity() {
  const order = ["tactical", "dense", "terminal"];
  state.density = order[(order.indexOf(state.density) + 1) % order.length];
  document.documentElement.dataset.density = state.density;
  $("#density-value").textContent = state.density.toUpperCase();
  globe.resize();
}
function updateVisibleCounts() {
  const visible = state.entities.filter(entityVisible);
  $("#visible-entities").textContent = String(visible.length).padStart(2, "0");
  $("#visible-links").textContent = String(uniqueLinks(visible).length).padStart(2, "0");
  $("#visible-sources").textContent = String(new Set(visible.flatMap((entity) => entity.source_ids)).size).padStart(2, "0");
}
function entityVisible(entity) { return [...state.activeLayers].some((layer) => TYPE_GROUPS[layer]?.has(entity.type.toUpperCase())); }

function openSearch(query = "") {
  const dialog = $("#search-dialog");
  if (!dialog.open) dialog.showModal();
  $("#search-input").value = query;
  $("#search-input").focus();
  executeSearch(query);
}
async function executeSearch(query) {
  state.searchResults = await client.search(query, { limit: 24 });
  state.searchCursor = 0;
  renderSearch();
}
function renderSearch() {
  const list = $("#search-results");
  list.replaceChildren();
  $("#search-result-count").textContent = `${state.searchResults.length} RESULT${state.searchResults.length === 1 ? "" : "S"}`;
  if (!state.searchResults.length) {
    const empty = document.createElement("div"); empty.className = "search-empty"; empty.textContent = "No public records match this query."; list.append(empty); return;
  }
  state.searchResults.forEach((result, index) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = `search-result${index === state.searchCursor ? " is-selected" : ""}`;
    button.innerHTML = `<em>${escapeHtml(result.type ?? result.kind ?? "RECORD")}</em><span><b>${escapeHtml(result.name ?? result.title ?? result.id)}</b><small>${escapeHtml(result.subtitle ?? "")}</small></span><strong>${result.source_count ?? 0} SRC</strong>`;
    button.addEventListener("click", () => activateResult(result));
    list.append(button);
  });
}
function searchKeys(event) {
  if (event.key === "ArrowDown") { event.preventDefault(); state.searchCursor = Math.min(state.searchResults.length - 1, state.searchCursor + 1); renderSearch(); }
  else if (event.key === "ArrowUp") { event.preventDefault(); state.searchCursor = Math.max(0, state.searchCursor - 1); renderSearch(); }
  else if (event.key === "Enter") { event.preventDefault(); const result = state.searchResults[state.searchCursor]; if (result) activateResult(result); }
}
function activateResult(result) {
  $("#search-dialog").close();
  result.kind === "source" || result.type === "SOURCE" ? openSource(result.id) : selectEntity(result.id);
}
