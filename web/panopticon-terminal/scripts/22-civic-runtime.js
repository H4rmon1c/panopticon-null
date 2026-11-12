function patchRuntime() {
  if (typeof window.renderActivity === "function") {
    const baseRenderActivity = window.renderActivity;
    window.renderActivity = function civicRenderActivity(...args) {
      const result = baseRenderActivity(...args);
      translateActivityLabels();
      return result;
    };
  }

  if (typeof window.renderDossier === "function") {
    const baseRenderDossier = window.renderDossier;
    window.renderDossier = function civicRenderDossier(...args) {
      const result = baseRenderDossier(...args);
      updateCivicContext(args[0]);
      return result;
    };
  }

  if (typeof window.selectEntity === "function") {
    const baseSelectEntity = window.selectEntity;
    window.selectEntity = async function civicSelectEntity(...args) {
      const result = await baseSelectEntity(...args);
      updateCivicContext(currentState().selected);
      return result;
    };
  }

  if (typeof window.toggleLayer === "function") {
    const baseToggleLayer = window.toggleLayer;
    window.toggleLayer = function civicToggleLayer(...args) {
      const result = baseToggleLayer(...args);
      atlas?.setLayers(currentState().activeLayers);
      return result;
    };
  }

  if (typeof window.setView === "function") {
    const baseSetView = window.setView;
    window.setView = function civicSetView(view, ...rest) {
      const result = baseSetView(view, ...rest);
      atlas?.setView(view === "network" ? "connections" : "place");
      setAtlasView(view !== "records");
      return result;
    };
  }

  if (typeof window.focusNetwork === "function") {
    const baseFocusNetwork = window.focusNetwork;
    window.focusNetwork = function civicFocusNetwork(...args) {
      const result = baseFocusNetwork(...args);
      atlas?.setView("connections");
      setAtlasView(true);
      return result;
    };
  }

  if (typeof window.resetGlobal === "function") {
    const baseResetGlobal = window.resetGlobal;
    window.resetGlobal = function civicResetGlobal(...args) {
      document.body.dataset.surface = "atlas";
      const result = baseResetGlobal(...args);
      atlas?.setView("place");
      return result;
    };
  }

  if (typeof window.cycleDensity === "function") {
    const baseCycleDensity = window.cycleDensity;
    window.cycleDensity = function civicCycleDensity(...args) {
      const result = baseCycleDensity(...args);
      const value = civic$("#density-value");
      if (value && value.textContent.trim() === "TACTICAL") value.textContent = "PUBLIC";
      return result;
    };
  }

  if (typeof window.setSensor === "function") {
    const baseSetSensor = window.setSensor;
    window.setSensor = function civicSetSensor(sensor, ...rest) {
      const result = baseSetSensor(sensor, ...rest);
      updateReadingMode(sensor);
      return result;
    };
  }

  if (typeof window.renderCommands === "function") {
    const baseRenderCommands = window.renderCommands;
    window.renderCommands = function civicRenderCommands(...args) {
      const result = baseRenderCommands(...args);
      rewriteCommands();
      return result;
    };
  }

  if (typeof window.boot === "function") {
    const baseBoot = window.boot;
    window.boot = async function civicBoot(...args) {
      const result = await baseBoot(...args);
      const snapshot = currentState();
      atlas?.setData(snapshot.entities ?? []);
      atlas?.setLayers(snapshot.activeLayers ?? new Set());
      atlas?.setSelected(snapshot.selected?.id);
      updateCivicContext(snapshot.selected);
      translateActivityLabels();
      rewriteCommands();
      updateReadingMode(snapshot.sensor ?? "record");
      renderRecordList();
      return result;
    };
  }
}

applyCivicShell();
installRecordList();
wireCivicUi();
atlas = new CivicAtlas(civic$("#civic-canvas"), { onSelect: selectRecord });
window.civicAtlas = atlas;
patchRuntime();

// Start with a readable list of released decisions. The relationship canvas is
// available on request and never stands in for the evidence record.
function installRecordList() {
  const canvas = civic$("#civic-canvas");
  if (!canvas) return;
  document.body.dataset.atlasView = "records";
  const panel = document.createElement("section");
  panel.className = "civic-record-browser";
  panel.setAttribute("aria-label", "Released decisions");
  panel.innerHTML = `<header><div><span class="eyebrow">SOURCE-VERIFIED RECORDS</span><h2>Explore recorded decisions</h2><p>Choose a decision to see the record and its sources. Coverage is incomplete.</p></div><button type="button" id="atlas-view-toggle" aria-pressed="false">SHOW CONNECTIONS MAP</button></header><div id="atlas-record-list" class="civic-record-list"></div>`;
  canvas.insertAdjacentElement("afterend", panel);
  const toggle = panel.querySelector("#atlas-view-toggle");
  toggle.addEventListener("click", () => setAtlasView(document.body.dataset.atlasView !== "map"));
}

function setAtlasView(showMap) {
  document.body.dataset.atlasView = showMap ? "map" : "records";
  const toggle = civic$("#atlas-view-toggle");
  if (toggle) {
    toggle.setAttribute("aria-pressed", String(showMap));
    toggle.textContent = showMap ? "SHOW DECISION LIST" : "SHOW CONNECTIONS MAP";
  }
  if (showMap) atlas?.resize();
}

function renderRecordList() {
  const list = civic$("#atlas-record-list");
  if (!list) return;
  list.replaceChildren();
  const entities = client.snapshot?.entities ?? currentState().entities ?? [];
  const byId = new Map(entities.map((entity) => [entity.id, entity]));
  const records = entities.filter((entity) => String(entity.type).toUpperCase() === "DECISION");
  if (!records.length) {
    const notice = document.createElement("p");
    notice.textContent = "No released decisions are available in this snapshot.";
    list.append(notice);
    return;
  }
  for (const record of records) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "civic-record-row";
    button.setAttribute("aria-pressed", String(record.id === currentState().selected?.id));
    const title = document.createElement("strong");
    title.textContent = record.name;
    const subtitle = document.createElement("span");
    const jurisdiction = (record.jurisdiction_ids ?? []).map((id) => byId.get(id)?.name).find(Boolean);
    subtitle.textContent = [jurisdiction, record.subtitle].filter(Boolean).join(" · ") || "Open the documented decision";
    button.append(title, subtitle);
    button.addEventListener("click", () => {
      list.querySelectorAll(".civic-record-row").forEach((row) => row.setAttribute("aria-pressed", String(row === button)));
      selectRecord(record.id);
      if (matchMedia("(max-width: 980px)").matches) {
        civic$(".dossier")?.scrollIntoView({ behavior: "auto", block: "start" });
      }
    });
    list.append(button);
  }
}
