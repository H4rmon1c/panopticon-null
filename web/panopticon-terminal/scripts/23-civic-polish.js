const originalCivicSummary = civicSummary;
civicSummary = function conciseCivicSummary(entity, snapshot) {
  const relationships = entity.relationships ?? [];
  const names = relationships
    .map((item) => snapshot.byId?.get?.(item.target_entity_id)?.name ?? item.target_entity_id)
    .filter(Boolean);
  const latest = [...(entity.timeline ?? [])].sort((a, b) => String(b.date).localeCompare(String(a.date)))[0];
  const preview = names.slice(0, 2);
  let connectionText = " has no published connections in this dataset yet.";
  if (preview.length === 1) connectionText = ` connects to ${preview[0]}.`;
  if (preview.length === 2) {
    const remainder = names.length - 2;
    connectionText = ` connects to ${preview[0]} and ${preview[1]}${remainder > 0 ? `, plus ${remainder} more public record${remainder === 1 ? "" : "s"}` : ""}.`;
  }
  const latestText = latest?.title ? ` Latest published change: ${latest.title}.` : "";
  return `${entity.name}${connectionText}${latestText}`;
};

const missionTitle = civic$("#mission-title");
if (missionTitle) missionTitle.textContent = "POWER LEAVES A RECORD. WE CONNECT IT.";

const privacyLine = civic$(".civic-place-privacy");
if (privacyLine) privacyLine.textContent = "No device location. No private-person tracking. Public records only.";

updateCivicContext(currentState().selected);
