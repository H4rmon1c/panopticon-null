import { PanopticonClient, startSnapshotRefresh } from "./api.js";
import { installColoradoCoverage } from "./scripts/24-colorado-coverage.mjs";
import { linkedDecisions, renderDecisionCards } from "./scripts/25-decision-cards.mjs";
window.renderDecisionCards = renderDecisionCards;
window.linkedDecisions = linkedDecisions;
window.atlasInitialHash = location.hash;
window.atlasLinkedEntity = new URLSearchParams(location.hash.slice(1)).get("entity");

const templateParts = [
  "./templates/shell-00.html",
  "./templates/shell-01.html",
  "./templates/shell-02.html",
  "./templates/shell-03.html",
];

const fragments = await Promise.all(templateParts.map(async (url) => {
  const response = await fetch(url, { cache: "no-store" });
  if (!response.ok) throw new Error(`Failed to load ${url} (${response.status})`);
  return response.text();
}));

document.body.innerHTML = fragments.join("");
document.title = "PANOPTICON.FAIL // The Public Record, Connected";
document.querySelector('meta[name="theme-color"]')?.setAttribute("content", "#071019");

const classicScripts = [
  "./scripts/00-core.js",
  "./scripts/12-helpers.js",
  "./scripts/01-boot.js",
  "./scripts/02-status-activity.js",
  "./scripts/03-dossier.js",
  "./scripts/04-evidence.js",
  "./scripts/05-view-search.js",
  "./scripts/06-commands.js",
  "./scripts/07-timeline.js",
  "./scripts/08-globe-base.js",
  "./scripts/09-globe-world.js",
  "./scripts/10-globe-data.js",
  "./scripts/11-globe-input.js",
  "./scripts/13-civic-data.js",
  "./scripts/14-civic-shell.js",
  "./scripts/15-civic-actions.js",
  "./scripts/16-civic-context.js",
  "./scripts/17-civic-atlas-core.js",
  "./scripts/18-civic-atlas-layout.js",
  "./scripts/19-civic-atlas-ground.js",
  "./scripts/20-civic-atlas-links.js",
  "./scripts/21-civic-atlas-helpers.js",
  "./scripts/22-civic-runtime.js",
  "./scripts/23-civic-polish.js",
];

for (const src of classicScripts) {
  await new Promise((resolve, reject) => {
    const script = document.createElement("script");
    script.src = src;
    script.async = false;
    script.addEventListener("load", resolve, { once: true });
    script.addEventListener("error", () => reject(new Error(`Failed to load ${src}`)), { once: true });
    document.head.append(script);
  });
}

installColoradoCoverage({
  getSnapshot: () => ({ mode: window.client?.mode, entities: window.client?.snapshot?.entities ?? [] }),
});

const params = new URLSearchParams(location.search);
window.client = new PanopticonClient({ demo: params.get("demo") === "1", preview: params.get("preview") === "1" });

window.boot().then(() => startSnapshotRefresh(window.client, { onError: () => {
  const label = document.querySelector("#system-state-label");
  if (label) label.textContent = "UPDATE CHECK FAILED // SHOWING LOADED SNAPSHOT";
  const notice = document.querySelector("#records-mode-notice");
  if (notice) {
    notice.hidden = false;
    notice.textContent = "The update check failed. Showing the last loaded records.";
  }
} })).catch((error) => {
  console.error(error);
  document.querySelector("#system-state")?.classList.add("is-stale");
  const label = document.querySelector("#system-state-label");
  if (label) label.textContent = "OFFLINE";
  const brief = document.querySelector("#civic-brief-text");
  if (brief) brief.textContent = "Reviewed public data is unavailable.";
  const count = document.querySelector("#records-count");
  if (count) count.textContent = "Decisions could not be loaded.";
  const list = document.querySelector("#atlas-record-list");
  if (list) {
    const message = document.createElement("p");
    message.className = "record-empty";
    message.textContent = "The public records could not be loaded. Please refresh this page and try again.";
    list.replaceChildren(message);
  }
  window.toast?.(`TERMINAL FAILED // ${error.message}`);
});
