function uniqueCivicLinks(entities) {
  if (typeof uniqueLinks === "function") return uniqueLinks(entities);
  const allowed = new Set(entities.map((entity) => entity.id));
  const seen = new Set();
  const output = [];
  entities.forEach((entity) => (entity.relationships ?? []).forEach((link) => {
    if (!allowed.has(link.target_entity_id)) return;
    const key = [entity.id, link.target_entity_id].sort().join("::") + `::${link.type}`;
    if (seen.has(key)) return;
    seen.add(key);
    output.push({ ...link, source_entity_id: entity.id });
  }));
  return output;
}

function linkColorCivic(type) {
  const value = String(type ?? "").toUpperCase();
  if (/POWER|UTILITY|EXECUT|GOVERN/.test(value)) return "#f3b84b";
  if (/NETWORK|CONNECT/.test(value)) return "#7fa7df";
  if (/BUILD|CONTRACT|SUPPL|VENDOR/.test(value)) return "#82d5a0";
  if (/VOTE|DECISION|PERMIT|ORDINANCE/.test(value)) return "#f0d579";
  return "#66ddea";
}

function pointOnPath(points, t) {
  const lengths = [];
  let total = 0;
  for (let index = 1; index < points.length; index += 1) {
    const length = Math.hypot(points[index].x - points[index - 1].x, points[index].y - points[index - 1].y);
    lengths.push(length);
    total += length;
  }
  let target = total * t;
  for (let index = 0; index < lengths.length; index += 1) {
    if (target <= lengths[index]) {
      const ratio = lengths[index] ? target / lengths[index] : 0;
      return {
        x: points[index].x + (points[index + 1].x - points[index].x) * ratio,
        y: points[index].y + (points[index + 1].y - points[index].y) * ratio,
      };
    }
    target -= lengths[index];
  }
  return points.at(-1);
}

function trimText(ctx, value, maxWidth) {
  if (ctx.measureText(value).width <= maxWidth) return value;
  let text = value;
  while (text.length > 3 && ctx.measureText(`${text}…`).width > maxWidth) text = text.slice(0, -1);
  return `${text}…`;
}

function rgbaCivic(hex, alpha) {
  const value = parseInt(hex.replace("#", ""), 16);
  return `rgba(${(value >> 16) & 255}, ${(value >> 8) & 255}, ${value & 255}, ${alpha})`;
}

function clampCivic(value, min, max) {
  return Math.min(max, Math.max(min, value));
}
