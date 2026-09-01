function renderStatus() {
  const counts = state.status?.counts ?? {};
  $("#graph-count").textContent = `${Number(counts.entities || state.entities.length).toLocaleString()} ENTITIES`;
  const node = $("#system-state");
  if (state.mode === "demo") {
    node.classList.add("is-demo");
    $("#system-state-label").textContent = "DEMO DATA";
  } else {
    const age = state.status?.last_publish ? Date.now() - new Date(state.status.last_publish).getTime() : Infinity;
    node.classList.toggle("is-stale", age > 3 * 60 * 60 * 1000);
    $("#system-state-label").textContent = age > 3 * 60 * 60 * 1000 ? "STALE" : "PUBLIC LIVE";
  }
  updateVisibleCounts();
}

function renderLayers() {
  $$(".layer").forEach((button) => {
    const group = TYPE_GROUPS[button.dataset.layer];
    button.querySelector("em").textContent = state.entities.filter((entity) => group.has(entity.type.toUpperCase())).length;
  });
}

function renderActivity() {
  const list = $("#activity-list");
  list.replaceChildren();
  state.activity.slice(0, 8).forEach((item, index) => {
    const entity = state.byId.get(item.entity_id);
    const button = document.createElement("button");
    button.type = "button";
    button.className = `activity-item${index === state.activityCursor ? " is-current" : ""}`;
    button.innerHTML = `<time>${escapeHtml(timeOnly(item.timestamp))}</time><em>${escapeHtml(item.type ?? "OBSERVED")}</em><span><b>${escapeHtml(item.summary ?? "Public record updated")}</b><small>${escapeHtml(entity?.name ?? item.status ?? "PUBLIC RECORD")}</small></span>`;
    button.addEventListener("click", () => item.entity_id && selectEntity(item.entity_id));
    list.append(button);
  });
}

function rotateActivity() {
  if (state.activityPaused || state.activity.length < 2) return;
  state.activityCursor = (state.activityCursor + 1) % Math.min(8, state.activity.length);
  renderActivity();
  const id = state.activity[state.activityCursor]?.entity_id;
  if (id) globe.ping(id);
}

function toggleActivity() {
  state.activityPaused = !state.activityPaused;
  $("#pause-activity").textContent = state.activityPaused ? "▶" : "Ⅱ";
}

async function selectEntity(id, focus = true, replaceHistory = false) {
  const entity = state.byId.get(id) ?? await client.getEntity(id);
  if (!entity.geo) entity.geo = GEO[entity.id] ?? fallbackGeo(0, 1);
  state.selected = entity;
  state.byId.set(entity.id, entity);
  globe.select(entity.id);
  if (focus) globe.focus(entity.geo, state.view === "network" ? 1.48 : 1.30);
  await loadSources(entity.source_ids);
  renderDossier(entity);
  const hash = `entity=${encodeURIComponent(entity.id)}`;
  if (location.hash.slice(1) !== hash) history[replaceHistory ? "replaceState" : "pushState"](null, "", `#${hash}`);
}

async function loadSources(ids) {
  const missing = [...new Set(ids)].filter((id) => id && !state.sources.has(id));
  const results = await Promise.allSettled(missing.map((id) => client.getSource(id)));
  results.forEach((result) => { if (result.status === "fulfilled") state.sources.set(result.value.id, result.value); });
}
