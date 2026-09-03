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
      return result;
    };
  }

  if (typeof window.focusNetwork === "function") {
    const baseFocusNetwork = window.focusNetwork;
    window.focusNetwork = function civicFocusNetwork(...args) {
      const result = baseFocusNetwork(...args);
      atlas?.setView("connections");
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
      return result;
    };
  }
}

applyCivicShell();
wireCivicUi();
atlas = new CivicAtlas(civic$("#civic-canvas"), { onSelect: selectRecord });
window.civicAtlas = atlas;
patchRuntime();

if (typeof window.boot !== "function") {
  atlas.setData(FALLBACK_ENTITIES);
  atlas.setLayers(currentState().activeLayers);
  atlas.setSelected("ent_front_range");
  updateCivicContext(FALLBACK_ENTITIES[0]);
  translateActivityLabels();
  rewriteCommands();
}
