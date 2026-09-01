import { PanopticonClient } from "./api.js";

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
document.title = "PANOPTICON.FAIL // Public Intelligence System";
document.querySelector('meta[name="theme-color"]')?.setAttribute("content", "#020508");

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

window.client = new PanopticonClient();

window.boot().catch((error) => {
  console.error(error);
  document.querySelector("#system-state")?.classList.add("is-stale");
  const label = document.querySelector("#system-state-label");
  if (label) label.textContent = "OFFLINE";
  window.toast?.(`TERMINAL FAILED // ${error.message}`);
});
