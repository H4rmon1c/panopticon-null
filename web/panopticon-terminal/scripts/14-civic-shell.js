function applyCivicShell() {
  if (civic$("#civic-canvas")) return;

  document.documentElement.dataset.experience = "civic";
  try {
    audience = localStorage.getItem("pnull-audience") === "reporter" ? "reporter" : "public";
  } catch {
    audience = "public";
  }
  document.documentElement.dataset.audience = audience;
  document.body.dataset.surface = "atlas";

  const app = civic$("#app");
  const world = civic$("#world-canvas");
  const canvas = document.createElement("canvas");
  canvas.id = "civic-canvas";
  canvas.setAttribute("aria-label", "Interactive public-record atlas showing documented institutional relationships");
  world?.insertAdjacentElement("afterend", canvas);

  const brandSub = civic$(".brand__sub");
  if (brandSub) brandSub.textContent = "THE PUBLIC RECORD, CONNECTED";
  const brand = civic$("#home-button");
  brand?.setAttribute("aria-label", "Return to the public record atlas");

  const publicIndexLabel = civic$(".telemetry--count span");
  if (publicIndexLabel) publicIndexLabel.textContent = "PUBLIC INDEX";
  const densityValue = civic$("#density-value");
  if (densityValue && densityValue.textContent.trim() === "TACTICAL") densityValue.textContent = "PUBLIC";

  const searchPrefix = civic$(".omnibox__prefix");
  if (searchPrefix) searchPrefix.textContent = "ASK";
  const searchText = civic$(".omnibox__text");
  if (searchText) searchText.textContent = "Search a town, agency, company, contract, vote, project, or source…";

  civic$(".topbar__right")?.insertAdjacentHTML("afterbegin", `
    <div class="audience-switch" role="group" aria-label="Choose public or reporter workspace">
      <button type="button" data-audience="public">PUBLIC</button>
      <button type="button" data-audience="reporter">REPORTER</button>
    </div>
  `);

  const missionEyebrow = civic$(".mission-bar .eyebrow");
  if (missionEyebrow) missionEyebrow.textContent = "A PUBLIC INTELLIGENCE COMMONS";
  const missionTitle = civic$("#mission-title");
  if (missionTitle) missionTitle.textContent = "EVERYTHING POWER FILES, FUNDS, BUILDS, OWNS, VOTES ON, AND CHANGES — CONNECTED.";
  const focusButton = civic$("#focus-network");
  if (focusButton) focusButton.textContent = "TRACE CONNECTIONS";

  civic$(".mission-bar")?.insertAdjacentHTML("afterend", `
    <section class="civic-place-panel" aria-labelledby="civic-place-title">
      <header class="civic-place-panel__head">
        <div><span class="eyebrow">START WITH A PLACE</span><h2 id="civic-place-title">What is happening here?</h2></div>
        <span>PLACE LENS</span>
      </header>
      <form id="civic-place-form" class="civic-place-form">
        <input id="civic-place-input" value="Colorado Springs, CO" autocomplete="off" aria-label="Town, county, or ZIP code" />
        <button type="submit">OPEN</button>
      </form>
      <div class="civic-question-grid" aria-label="Common public questions">
        <button type="button" data-civic-action="changes">WHAT CHANGED?</button>
        <button type="button" data-civic-action="money">WHERE DID MONEY GO?</button>
        <button type="button" data-civic-action="decisions">WHO APPROVED IT?</button>
        <button type="button" data-civic-action="building">WHAT IS BEING BUILT?</button>
      </div>
      <p class="civic-place-privacy">Manual place search. No device-location access.</p>
    </section>
  `);

  app?.insertAdjacentHTML("beforeend", `
    <div class="civic-atlas-label" aria-hidden="true">
      <span>PUBLIC POWER MAP</span>
      <strong id="civic-atlas-place">COLORADO SPRINGS, CO // DEMO</strong>
    </div>
    <section class="civic-principle" aria-live="polite">
      <span>CURRENT PUBLIC BRIEF</span>
      <strong id="civic-brief-text">Records connect a major project, its operator, utility, builder, network provider, and governing agreement.</strong>
      <p id="civic-brief-proof">Known facts are sourced. Missing facts are shown as unknown, never guessed.</p>
    </section>
  `);

  relabelPrimaryUi();
  injectCivicLayers();
  injectDossierContext();
  relabelDialogs();
  updateAudienceUi();
}

function relabelPrimaryUi() {
  const layerHeader = civic$(".layer-panel .panel__header");
  const layerEyebrow = layerHeader?.querySelector(".eyebrow");
  const layerTitle = layerHeader?.querySelector("h2");
  if (layerEyebrow) layerEyebrow.textContent = "LAYERS OF POWER";
  if (layerTitle) layerTitle.textContent = "What shapes this place";

  const viewNames = { global: ["⌖", "PLACE"], network: ["⌁", "CONNECTIONS"], records: ["▤", "RECORDS"] };
  civic$$(".view-tab").forEach((button) => {
    const [glyph, label] = viewNames[button.dataset.view] ?? ["·", button.dataset.view];
    const icon = button.querySelector("i");
    const text = button.querySelector("span");
    if (icon) icon.textContent = glyph;
    if (text) text.textContent = label;
  });

  const layerNames = {
    FACILITY: ["PROJECTS & PLACES", "sites, permits, campuses"],
    ORGANIZATION: ["AGENCIES & OWNERS", "public bodies and operators"],
    UTILITY: ["POWER & UTILITIES", "grid plans and agreements"],
    NETWORK: ["NETWORKS", "carriers and infrastructure"],
    CONTRACTOR: ["VENDORS & BUILDERS", "who does and supplies the work"],
    CONTRACT: ["MONEY & CONTRACTS", "awards, bids, obligations"],
  };
  civic$$(".layer").forEach((button) => {
    const entry = layerNames[button.dataset.layer];
    if (!entry) return;
    const bold = button.querySelector("b");
    const small = button.querySelector("small");
    if (bold) bold.textContent = entry[0];
    if (small) small.textContent = entry[1];
  });

  const sensorBlock = civic$(".sensor-block");
  const sensorEyebrow = sensorBlock?.querySelector(".eyebrow");
  if (sensorEyebrow) sensorEyebrow.textContent = "READING MODE";
  const sensorNames = { record: "SUMMARY", night: "SOURCE", change: "CHANGES" };
  civic$$(".sensor-tab").forEach((button) => { button.textContent = sensorNames[button.dataset.sensor] ?? button.textContent; });
  const sensorLabel = civic$("#sensor-label");
  if (sensorLabel) sensorLabel.textContent = "PLAIN-LANGUAGE VIEW";
  const sensorNote = sensorBlock?.querySelector("p");
  if (sensorNote) sensorNote.textContent = "Change how records are read, not what the evidence says.";

  const activityHeader = civic$(".activity-panel .panel__header");
  const activityEyebrow = activityHeader?.querySelector(".eyebrow");
  const activityTitle = activityHeader?.querySelector("h2");
  if (activityEyebrow) activityEyebrow.textContent = "WHAT CHANGED";
  if (activityTitle) activityTitle.textContent = "New in this place";

  const classification = civic$(".dossier__classification span");
  if (classification) classification.textContent = "PUBLIC RECORD // PROOF ATTACHED";
  const dossierEmptyEyebrow = civic$("#dossier-empty .eyebrow");
  if (dossierEmptyEyebrow) dossierEmptyEyebrow.textContent = "NO RECORD SELECTED";
  const dossierEmptyText = civic$("#dossier-empty p");
  if (dossierEmptyText) dossierEmptyText.textContent = "Choose a place, public record, documented connection, or source.";

  const dossierTabs = civic$$(".dossier-tabs button");
  if (dossierTabs[0]) dossierTabs[0].textContent = "IN PLAIN ENGLISH";
  if (dossierTabs[1]) dossierTabs[1].textContent = "CONNECTIONS";
  if (dossierTabs[2]) dossierTabs[2].textContent = "PROOF";

  const primary = civic$("#primary-evidence");
  if (primary) primary.textContent = "READ THE PROOF";
  const permalink = civic$("#copy-permalink");
  if (permalink) permalink.textContent = "SHARE THIS RECORD";

  const timelineEyebrow = civic$(".timeline__transport .eyebrow");
  if (timelineEyebrow) timelineEyebrow.textContent = "PUBLIC RECORD HISTORY";
  const timelineState = civic$("#timeline-state");
  if (timelineState) timelineState.textContent = "LATEST PUBLISHED RECORD";

  const evidenceHeader = civic$(".evidence-drawer header .eyebrow");
  if (evidenceHeader) evidenceHeader.textContent = "SOURCE PROOF";
  const evidenceLink = civic$("#evidence-link");
  if (evidenceLink) evidenceLink.textContent = "OPEN ORIGINAL PUBLIC SOURCE ↗";
  const evidenceNote = civic$(".evidence-note");
  if (evidenceNote) evidenceNote.textContent = "A connection is never stronger than the public evidence attached to it.";
}
