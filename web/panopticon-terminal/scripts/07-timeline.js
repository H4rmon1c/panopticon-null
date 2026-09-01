function renderTimeline() {
  state.timelineEvents = state.entities.flatMap((entity) => entity.timeline.map((event) => ({ ...event, entity_id: entity.id }))).sort((a,b) => new Date(a.date) - new Date(b.date));
  const events = state.timelineEvents;
  if (!events.length) return;
  const start = new Date(events[0].date).getTime();
  const end = new Date(events.at(-1).date).getTime();
  $("#timeline-range").textContent = `${shortDate(start)} — ${shortDate(end)}`;
  const container = $("#timeline-events");
  events.forEach((event) => {
    const position = end === start ? 50 : ((new Date(event.date).getTime() - start) / (end - start)) * 100;
    const marker = document.createElement("button");
    marker.type = "button"; marker.className = "timeline-event"; marker.style.left = `${position}%`; marker.dataset.label = event.type; marker.title = `${event.date} · ${event.title}`;
    marker.addEventListener("click", () => { $("#timeline-input").value = String(Math.round(position * 10)); scrubTimeline(); selectEntity(event.entity_id); globe.ping(event.entity_id); });
    container.append(marker);
  });
}
function scrubTimeline() {
  const value = Number($("#timeline-input").value);
  $("#timeline-progress").style.width = `calc(${value / 10}% - 18px)`;
  if (!state.timelineEvents.length) return;
  const start = new Date(state.timelineEvents[0].date).getTime();
  const end = new Date(state.timelineEvents.at(-1).date).getTime();
  const current = start + (end - start) * value / 1000;
  $("#timeline-current").textContent = dateTime(current).replace(" UTC", "");
  $("#timeline-state").textContent = value > 994 ? "LATEST VERIFIED PUBLICATION" : "HISTORICAL PUBLIC VIEW";
  $$(".timeline-event").forEach((node, index) => node.classList.toggle("is-active", Math.abs(new Date(state.timelineEvents[index].date).getTime() - current) < Math.max(259200000, (end-start)/20)));
  globe.setTimeline(value / 1000);
}
function toggleTimeline() {
  const button = $("#timeline-play");
  if (timelineTimer) { clearInterval(timelineTimer); timelineTimer = null; button.textContent = "▶"; return; }
  if (Number($("#timeline-input").value) >= 1000) $("#timeline-input").value = "0";
  button.textContent = "Ⅱ";
  timelineTimer = setInterval(() => {
    const input = $("#timeline-input");
    const next = Number(input.value) + 8;
    input.value = String(Math.min(1000, next)); scrubTimeline();
    if (next >= 1000) toggleTimeline();
  }, 55);
}
