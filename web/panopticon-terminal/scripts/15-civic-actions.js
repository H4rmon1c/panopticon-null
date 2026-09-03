function injectCivicLayers() {
  if (typeof TYPE_GROUPS !== "undefined") {
    TYPE_GROUPS.DECISION = new Set(["DECISION", "VOTE", "ORDINANCE", "PERMIT", "MEETING"]);
    TYPE_GROUPS.PERSON = new Set(["PERSON", "OFFICIAL", "OFFICEHOLDER"]);
  }
  if (typeof TYPE_STYLE !== "undefined") {
    TYPE_STYLE.DECISION = ["#f0d579", "DEC", "diamond"];
    TYPE_STYLE.VOTE = ["#f0d579", "VOTE", "diamond"];
    TYPE_STYLE.PERSON = ["#d8e0e2", "ROLE", "circle"];
    TYPE_STYLE.OFFICIAL = ["#d8e0e2", "ROLE", "circle"];
  }
  const snapshot = currentState();
  snapshot.activeLayers?.add?.("DECISION");
  snapshot.activeLayers?.add?.("PERSON");

  const list = civic$("#layer-list");
  if (!list || civic$("[data-layer='DECISION']", list)) return;
  list.insertAdjacentHTML("beforeend", `
    <button class="layer is-active" data-layer="DECISION" type="button">
      <i class="layer-dot layer-dot--decision"></i><span><b>DECISIONS & VOTES</b><small>permits, meetings, ordinances</small></span><em>0</em>
    </button>
    <button class="layer is-active" data-layer="PERSON" type="button">
      <i class="layer-dot layer-dot--person"></i><span><b>PEOPLE IN POWER</b><small>documented public roles only</small></span><em>0</em>
    </button>
  `);
}

function injectDossierContext() {
  const pane = civic$(".dossier-pane[data-pane='dossier']");
  const description = civic$("#entity-description");
  if (pane && description && !civic$("#civic-context")) {
    description.insertAdjacentHTML("beforebegin", `
      <section id="civic-context" class="civic-context">
        <h3>WHY THIS MATTERS</h3>
        <p id="civic-why-text">Select a record to see the public stakes in plain language.</p>
      </section>
    `);
    civic$("#attribute-list")?.insertAdjacentHTML("afterend", `
      <section class="civic-unknowns">
        <h3>WHAT THE PUBLIC RECORD DOES NOT YET SHOW</h3>
        <ul id="civic-unknown-list"><li>Unknowns will be listed here instead of filled with inference.</li></ul>
      </section>
      <section class="reporter-tools" aria-label="Reporter tools">
        <h3>REPORTER WORKBENCH</h3>
        <div class="reporter-tools__grid">
          <button id="export-evidence" type="button">EXPORT EVIDENCE PACK</button>
          <button id="copy-citations" type="button">COPY CITATION PACK</button>
          <button id="advanced-query" type="button">ADVANCED QUERY</button>
          <button id="wide-area-view" type="button">WIDE-AREA LENS</button>
        </div>
      </section>
    `);
  }
}

function relabelDialogs() {
  const searchInput = civic$("#search-input");
  if (searchInput) searchInput.placeholder = "Ask about a place, agency, company, contract, vote, project, or source…";
  const searchTitle = civic$("#search-title");
  if (searchTitle) searchTitle.textContent = "SEARCH THE PUBLIC RECORD";

  const hints = civic$$(".query-hints button");
  const hintLabels = [
    ["type:facility colorado", "What is being built?"],
    ["type:contract power", "Where did money or obligations move?"],
    ["source:official", "Show official sources"],
    ["Front Range", "Trace this local project"],
  ];
  hints.forEach((button, index) => {
    if (!hintLabels[index]) return;
    button.dataset.query = hintLabels[index][0];
    button.textContent = hintLabels[index][1];
  });

  const commandTitle = civic$("#command-title");
  if (commandTitle) commandTitle.textContent = "PUBLIC INTELLIGENCE COMMANDS";
}

function wireCivicUi() {
  civic$$(".audience-switch button").forEach((button) => {
    button.addEventListener("click", () => setAudience(button.dataset.audience));
  });

  civic$("#civic-place-form")?.addEventListener("submit", (event) => {
    event.preventDefault();
    const value = civic$("#civic-place-input")?.value.trim();
    if (!value) return;
    place = value.toUpperCase();
    updatePlaceLabel();
    if (/colorado springs/i.test(value) && currentState().byId?.has?.("ent_front_range")) {
      selectRecord("ent_front_range");
    } else if (typeof openSearch === "function") {
      openSearch(value);
    }
  });

  civic$$("[data-civic-action]").forEach((button) => {
    button.addEventListener("click", () => runCivicAction(button.dataset.civicAction));
  });

  civic$("#export-evidence")?.addEventListener("click", exportEvidencePacket);
  civic$("#copy-citations")?.addEventListener("click", copyCitationPack);
  civic$("#advanced-query")?.addEventListener("click", () => typeof openSearch === "function" && openSearch("source:official"));
  civic$("#wide-area-view")?.addEventListener("click", toggleWideArea);
}

function setAudience(next) {
  audience = next === "reporter" ? "reporter" : "public";
  document.documentElement.dataset.audience = audience;
  try { localStorage.setItem("pnull-audience", audience); } catch { /* storage may be disabled */ }
  updateAudienceUi();
  if (audience === "public" && document.body.dataset.surface === "wide") toggleWideArea(false);
  announce(audience === "reporter" ? "REPORTER WORKBENCH ENABLED" : "PUBLIC VIEW ENABLED");
}

function updateAudienceUi() {
  civic$$(".audience-switch button").forEach((button) => button.classList.toggle("is-active", button.dataset.audience === audience));
}

function runCivicAction(action) {
  const snapshot = currentState();
  if (action === "changes") {
    civic$(".activity-panel")?.classList.add("is-emphasized");
    setTimeout(() => civic$(".activity-panel")?.classList.remove("is-emphasized"), 900);
    announce("SHOWING RECENT PUBLISHED CHANGES");
    return;
  }
  if (action === "money") {
    const contract = snapshot.entities?.find((item) => /CONTRACT|AGREEMENT|AWARD/.test(String(item.type).toUpperCase()));
    contract ? selectRecord(contract.id) : typeof openSearch === "function" && openSearch("type:contract");
    return;
  }
  if (action === "decisions") {
    if (typeof setDossierTab === "function") setDossierTab("sources");
    if (typeof openSearch === "function") openSearch("source:official");
    return;
  }
  if (action === "building") {
    const facility = snapshot.entities?.find((item) => /FACILITY|PROJECT|CAMPUS/.test(String(item.type).toUpperCase()));
    facility ? selectRecord(facility.id) : typeof openSearch === "function" && openSearch("type:facility");
  }
}

function selectRecord(id) {
  if (typeof selectEntity === "function") selectEntity(id);
}

function updatePlaceLabel() {
  const label = civic$("#civic-atlas-place");
  if (label) label.textContent = `${place} // ${currentState().mode === "demo" ? "DEMO" : "PUBLIC RECORD"}`;
}
