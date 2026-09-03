async function boot() {
  wireUi();
  globe = new Globe($("#world-canvas"), {
    onSelect: (id) => selectEntity(id, false),
    onCamera: ({ lat, lon, altitude }) => {
      $("#camera-readout").textContent = `${coord(lat, "N", "S")} / ${coord(lon, "E", "W")}`;
      $("#altitude-readout").textContent = `${Math.round(altitude).toLocaleString()} KM`;
    },
  });

  const bootstrap = await client.bootstrap();
  state.mode = bootstrap.mode;
  state.status = bootstrap.status;
  state.activity = bootstrap.activity ?? [];

  const summaries = await client.search("", { limit: 100 });
  const ids = [...new Set([
    ...(bootstrap.featured ?? []).map((item) => item.id),
    ...summaries.filter((item) => item.kind !== "source").map((item) => item.id),
  ])].filter(Boolean);
  const hydrated = await client.getEntitiesByIds(ids);
  const fallback = (bootstrap.featured ?? []).filter((item) => item.relationships || item.attributes);
  state.entities = normalizeEntities(hydrated.length ? hydrated : fallback);
  state.byId = new Map(state.entities.map((entity) => [entity.id, entity]));

  globe.setData(state.entities);
  renderStatus();
  renderLayers();
  renderActivity();
  renderTimeline();
  renderCommands();

  const requested = new URLSearchParams(location.hash.slice(1)).get("entity");
  const initial = state.byId.has(requested)
    ? requested
    : state.byId.has("ent_front_range")
      ? "ent_front_range"
      : state.entities[0]?.id;
  if (initial) await selectEntity(initial, true, true);

  if (!new URLSearchParams(location.search).has("ci")) setInterval(rotateActivity, 3300);
}

function normalizeEntities(items) {
  return items.map((entity, index) => ({
    ...entity,
    geo: validGeo(entity.geo) ? entity.geo : GEO[entity.id] ?? fallbackGeo(index, items.length),
    attributes: entity.attributes ?? [],
    relationships: entity.relationships ?? [],
    timeline: entity.timeline ?? [],
    source_ids: entity.source_ids ?? [],
    tags: entity.tags ?? [],
  }));
}

function wireUi() {
  $("#home-button").addEventListener("click", resetGlobal);
  $("#global-search").addEventListener("click", () => openSearch());
  $("#command-button").addEventListener("click", openCommands);
  $("#close-command").addEventListener("click", () => $("#command-dialog").close());
  $("#density-button").addEventListener("click", cycleDensity);
  $("#focus-network").addEventListener("click", focusNetwork);
  $("#pause-activity").addEventListener("click", toggleActivity);
  $("#primary-evidence").addEventListener("click", openPrimaryEvidence);
  $("#copy-permalink").addEventListener("click", copyPermalink);
  $("#close-evidence").addEventListener("click", closeEvidence);
  $("#copy-hash").addEventListener("click", copyHash);
  $("#timeline-play").addEventListener("click", toggleTimeline);
  $("#timeline-input").addEventListener("input", scrubTimeline);

  $$(".view-tab").forEach((button) => button.addEventListener("click", () => setView(button.dataset.view)));
  $$(".layer").forEach((button) => button.addEventListener("click", () => toggleLayer(button)));
  $$(".sensor-tab").forEach((button) => button.addEventListener("click", () => setSensor(button.dataset.sensor)));
  $$(".dossier-tabs button").forEach((button) => button.addEventListener("click", () => setDossierTab(button.dataset.tab)));
  $$(".query-hints button").forEach((button) => button.addEventListener("click", () => openSearch(button.dataset.query)));

  const searchDialog = $("#search-dialog");
  const searchInput = $("#search-input");
  searchInput.addEventListener("input", () => {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(() => executeSearch(searchInput.value), 70);
  });
  searchInput.addEventListener("keydown", searchKeys);
  searchDialog.addEventListener("click", (event) => { if (event.target === searchDialog) searchDialog.close(); });
  $("#command-dialog").addEventListener("click", (event) => { if (event.target === $("#command-dialog")) $("#command-dialog").close(); });
  document.addEventListener("keydown", globalKeys);
  window.addEventListener("hashchange", () => {
    const id = new URLSearchParams(location.hash.slice(1)).get("entity");
    if (id && state.byId.has(id) && id !== state.selected?.id) selectEntity(id, true, true);
  });
}
