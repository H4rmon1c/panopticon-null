function renderCommands() {
  const commands = [
    ["⌕", "Search public graph", "Entities, facilities, contracts, sources", "/", () => openSearch()],
    ["◉", "Reset global view", "Return to the continental intelligence picture", "H", resetGlobal],
    ["⌁", "Focus selected network", "Show the immediate documented relationship chain", "G", focusNetwork],
    ["▣", "Open primary evidence", "Inspect exact source text and digest", "E", openPrimaryEvidence],
    ["▶", "Play record timeline", "Move through published event history", "T", toggleTimeline],
    ["↕", "Cycle display density", "Tactical, dense, and terminal modes", "D", cycleDensity],
    ["1", "Public record optics", "Neutral source-grounded view", "1", () => setSensor("record")],
    ["2", "Low-light optics", "Green low-light presentation", "2", () => setSensor("night")],
    ["3", "Change-intensity optics", "Highlight active and revised records", "3", () => setSensor("change")],
  ];
  const list = $("#command-list");
  commands.forEach(([icon, title, sub, key, run]) => {
    const button = document.createElement("button");
    button.type = "button"; button.className = "command-item";
    button.innerHTML = `<i>${escapeHtml(icon)}</i><span><b>${escapeHtml(title)}</b><small>${escapeHtml(sub)}</small></span><kbd>${escapeHtml(key)}</kbd>`;
    button.addEventListener("click", () => { $("#command-dialog").close(); run(); });
    list.append(button);
  });
}
function openCommands() { if (!$("#command-dialog").open) $("#command-dialog").showModal(); }
function globalKeys(event) {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") { event.preventDefault(); openCommands(); return; }
  if (/INPUT|TEXTAREA|SELECT/.test(document.activeElement?.tagName ?? "")) return;
  const key = event.key.toLowerCase();
  if (event.key === "/") { event.preventDefault(); openSearch(); }
  else if (key === "g") focusNetwork();
  else if (key === "h") resetGlobal();
  else if (key === "e") openPrimaryEvidence();
  else if (key === "t") toggleTimeline();
  else if (key === "d") cycleDensity();
  else if (event.key === "1") setSensor("record");
  else if (event.key === "2") setSensor("night");
  else if (event.key === "3") setSensor("change");
  else if (event.key === "[") stepEntity(-1);
  else if (event.key === "]") stepEntity(1);
  else if (event.key === "Escape") closeEvidence();
}
function stepEntity(direction) {
  if (!state.entities.length) return;
  const current = Math.max(0, state.entities.findIndex((entity) => entity.id === state.selected?.id));
  selectEntity(state.entities[(current + direction + state.entities.length) % state.entities.length].id);
}
