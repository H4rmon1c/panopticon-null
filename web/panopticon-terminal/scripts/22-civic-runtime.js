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
      // The old canvas selects an arbitrary first entity at boot. Do not turn
      // a clean list URL into a misleading permalink for that entity.
      if (!window.atlasInitialHash && location.hash.startsWith("#entity=")) {
        history.replaceState(null, "", `${location.pathname}${location.search}`);
      }
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
  panel.setAttribute("role", "main");
  panel.id = "decisions-main";
  panel.tabIndex = -1;
  panel.innerHTML = `<div class="records-topbar"><a class="records-brand" href="./home.html"><span>P//F</span> PANOPTICON.FAIL</a><nav aria-label="Main navigation"><a href="./home.html">Home</a><a href="./research.html">Research leads</a><button type="button" id="atlas-view-toggle" aria-pressed="false">Explore connections</button></nav></div>
    <div class="records-content"><header class="records-intro"><p class="records-eyebrow">COLORADO · PUBLIC RECORDS</p><h1>See what your community voted on.</h1><p>Start with a decision. See the recorded vote, the people involved, and the sources behind it.</p></header>
    <div id="records-mode-notice" class="records-mode-notice" hidden></div><aside id="records-linked" class="records-linked" hidden></aside>
    <section class="records-search" aria-label="Find a decision or place"><label for="records-query">Find a place, person, or topic</label><div class="records-search__row"><input type="search" id="records-query" placeholder="Try Castle Rock, Flock, or a name" autocomplete="off"><button type="button" id="records-place-search">Search all Colorado places</button></div><p>Search released decisions here, or open the full place directory for more places and preliminary leads.</p></section>
    <div class="records-heading"><div><h2>Released decisions</h2><p id="records-count" role="status" aria-live="polite">Loading decisions…</p></div><a href="./research.html">Browse preliminary research →</a></div>
    <p class="records-caveat">Coverage is incomplete. A place missing from this list may still have relevant records.</p><div id="atlas-record-list" class="civic-record-list"></div></div>`;
  canvas.insertAdjacentElement("afterend", panel);
  const toggle = panel.querySelector("#atlas-view-toggle");
  toggle.addEventListener("click", () => setAtlasView(document.body.dataset.atlasView !== "map"));
  const query = panel.querySelector("#records-query");
  query.addEventListener("input", () => renderRecordList());
  panel.querySelector("#records-place-search").addEventListener("click", () => {
    const value = query.value.trim();
    if (value) window.dispatchEvent(new CustomEvent("pnull:find-place", {detail:{query:value}}));
    else civic$("#coverage-open")?.click();
  });
  window.addEventListener("hashchange", () => {
    window.atlasLinkedEntity = new URLSearchParams(location.hash.slice(1)).get("entity");
    renderRecordList();
  });
  setAtlasView(false);
}

function setAtlasView(showMap) {
  document.body.dataset.atlasView = showMap ? "map" : "records";
  if (document.documentElement) document.documentElement.dataset.atlasView = showMap ? "map" : "records";
  const skip = document.querySelector(".skip-link");
  if (skip) {
    skip.href = showMap ? "#entity-workspace" : "#decisions-main";
    skip.textContent = showMap ? "Skip to selected record" : "Skip to decisions";
  }
  const toggle = civic$("#atlas-view-toggle");
  if (toggle) {
    toggle.setAttribute("aria-pressed", String(showMap));
    toggle.textContent = showMap ? "Back to decisions" : "Explore connections";
  }
  if (showMap) atlas?.resize();
}

function renderRecordList() {
  const list = civic$("#atlas-record-list");
  if (!list || !window.renderDecisionCards) return;
  const snapshot = client.snapshot ?? { entities: currentState().entities ?? [] };
  const mode = currentState().mode ?? client.mode ?? "public";
  const notice = civic$("#records-mode-notice");
  if (notice) {
    notice.hidden = mode === "public";
    notice.textContent = mode === "demo" ? "SYNTHETIC DEMONSTRATION — these are not real records." : "PRIVATE PREVIEW — these records may not be released.";
  }
  window.renderDecisionCards(snapshot, mode, civic$("#records-query")?.value ?? "", list, civic$("#records-count"));
  const linked = civic$("#records-linked");
  if (linked && window.linkedDecisions) {
    linked.replaceChildren();
    const entity = snapshot.entities?.find((item) => item.id === window.atlasLinkedEntity);
    linked.hidden = !entity;
    if (entity) {
      const title = document.createElement("strong");
      title.textContent = `You opened a link to ${entity.name ?? "a record"}.`;
      linked.append(title);
      const matches = window.linkedDecisions(snapshot, mode, entity.id);
      for (const match of matches) {
        const link = document.createElement("a");
        link.href = match.href;
        link.textContent = `${match.card.record.id === entity.id ? "View decision" : "View recorded position"} — ${match.card.title}`;
        linked.append(link);
      }
      if (!matches.length) {
        const button = document.createElement("button");
        button.type = "button";
        button.textContent = "Open this record in connections";
        button.addEventListener("click", () => { selectRecord(entity.id); setAtlasView(true); });
        linked.append(button);
      }
    }
  }
}
