CivicAtlas.prototype.drawLinks = function drawLinks(ctx, now) {
  const nodeById = new Map(this.nodes.map((node) => [node.entity.id, node]));
  const links = uniqueCivicLinks(this.nodes.map((node) => node.entity));
  links.forEach((link, index) => {
    const sourceNode = nodeById.get(link.source_entity_id);
    const targetNode = nodeById.get(link.target_entity_id);
    if (!sourceNode || !targetNode) return;
    if (this.view === "connections" && this.selected && link.source_entity_id !== this.selected && link.target_entity_id !== this.selected) return;
    const a = this.point(sourceNode.nx, sourceNode.ny);
    const b = this.point(targetNode.nx, targetNode.ny);
    const selected = link.source_entity_id === this.selected || link.target_entity_id === this.selected;
    const color = linkColorCivic(link.type);
    const bendX = a.x + (b.x - a.x) * 0.52;
    const path = [a, { x: bendX, y: a.y }, { x: bendX, y: b.y }, b];

    ctx.save();
    ctx.beginPath();
    ctx.moveTo(path[0].x, path[0].y);
    path.slice(1).forEach((point) => ctx.lineTo(point.x, point.y));
    ctx.strokeStyle = rgbaCivic(color, selected ? 0.72 : 0.28);
    ctx.lineWidth = selected ? 1.6 : 0.85;
    ctx.stroke();

    const pulse = pointOnPath(path, (now * 0.00008 + index * 0.19) % 1);
    ctx.fillStyle = color;
    ctx.shadowColor = color;
    ctx.shadowBlur = selected ? 12 : 7;
    ctx.fillRect(pulse.x - 1.5, pulse.y - 1.5, selected ? 4 : 3, selected ? 4 : 3);
    ctx.shadowBlur = 0;

    const label = String(link.label ?? link.type ?? "DOCUMENTED").toUpperCase();
    ctx.font = "700 6px ui-monospace, monospace";
    const width = ctx.measureText(label).width + 10;
    const lx = bendX - width / 2;
    const ly = (a.y + b.y) / 2 - 8;
    ctx.fillStyle = "rgba(6, 15, 22, .88)";
    ctx.fillRect(lx, ly, width, 14);
    ctx.strokeStyle = rgbaCivic(color, selected ? 0.38 : 0.16);
    ctx.strokeRect(lx, ly, width, 14);
    ctx.fillStyle = selected ? color : "rgba(163,185,193,.58)";
    ctx.fillText(label, lx + 5, ly + 9.5);
    ctx.restore();
  });
};

CivicAtlas.prototype.drawNodes = function drawNodes(ctx, now) {
  this.hits = [];
  const sorted = [...this.nodes].sort((a, b) => Number(a.entity.id === this.selected) - Number(b.entity.id === this.selected));
  sorted.forEach((node, index) => {
    const entity = node.entity;
    const p = this.point(node.nx, node.ny);
    const type = String(entity.type ?? "ENTITY").toUpperCase();
    const meta = TYPE_META[type] ?? { color: "#9bb3bc", short: "REC", title: "Record" };
    const selected = entity.id === this.selected;
    const cardW = selected ? 178 : 132;
    const cardH = selected ? 56 : 42;
    const x = p.x - cardW / 2;
    const y = p.y - cardH / 2;

    ctx.save();
    if (selected) {
      const pulse = this.reducedMotion ? 0.25 : (Math.sin(now * 0.003) + 1) / 2;
      ctx.strokeStyle = rgbaCivic(meta.color, 0.18 + pulse * 0.18);
      ctx.lineWidth = 1;
      ctx.strokeRect(x - 7 - pulse * 3, y - 7 - pulse * 3, cardW + 14 + pulse * 6, cardH + 14 + pulse * 6);
    }

    const fill = ctx.createLinearGradient(x, y, x + cardW, y + cardH);
    fill.addColorStop(0, selected ? "rgba(12, 30, 41, .98)" : "rgba(8, 20, 29, .94)");
    fill.addColorStop(1, "rgba(4, 12, 18, .96)");
    ctx.fillStyle = fill;
    ctx.strokeStyle = rgbaCivic(meta.color, selected ? 0.72 : 0.28);
    ctx.lineWidth = selected ? 1.35 : 0.9;
    ctx.fillRect(x, y, cardW, cardH);
    ctx.strokeRect(x, y, cardW, cardH);

    ctx.fillStyle = meta.color;
    ctx.fillRect(x, y, selected ? 4 : 3, cardH);
    ctx.fillRect(x + 11, y + 10, selected ? 8 : 6, selected ? 8 : 6);
    ctx.strokeStyle = rgbaCivic(meta.color, 0.42);
    ctx.strokeRect(x + 8, y + 7, selected ? 14 : 12, selected ? 14 : 12);

    ctx.fillStyle = "#edf3f3";
    ctx.font = `${selected ? 800 : 700} ${selected ? 11 : 8}px ui-monospace, monospace`;
    ctx.fillText(trimText(ctx, String(entity.name ?? entity.id).toUpperCase(), cardW - 42), x + 30, y + (selected ? 21 : 17));
    ctx.fillStyle = meta.color;
    ctx.font = "800 6px ui-monospace, monospace";
    ctx.fillText(`${meta.short} // ${Number(entity.source_count ?? entity.source_ids?.length ?? 0)} PUBLIC SOURCES`, x + 30, y + (selected ? 37 : 30));
    if (selected) {
      ctx.fillStyle = "rgba(178, 199, 206, .65)";
      ctx.font = "700 6px ui-monospace, monospace";
      ctx.fillText("CLICK A FACT OR CONNECTION TO OPEN ITS PROOF", x + 30, y + 48);
    }

    this.hits.push({ id: entity.id, x, y, w: cardW, h: cardH, index });
    ctx.restore();
  });
};

CivicAtlas.prototype.drawFurniture = function drawFurniture(ctx) {
  ctx.save();
  ctx.fillStyle = "rgba(146, 176, 187, .52)";
  ctx.font = "700 7px ui-monospace, monospace";
  ctx.fillText("DOCUMENTED PUBLIC RELATIONSHIPS", 18, this.height - 19);
  const width = ctx.measureText("DOCUMENTED PUBLIC RELATIONSHIPS").width;
  ctx.strokeStyle = "rgba(111, 225, 241, .18)";
  ctx.beginPath(); ctx.moveTo(18 + width + 10, this.height - 22); ctx.lineTo(this.width - 18, this.height - 22); ctx.stroke();

  ctx.fillStyle = "rgba(146, 176, 187, .42)";
  ctx.fillText("NOT LIVE TRACKING // PUBLIC RECORDS ONLY", Math.max(18, this.width - 210), 20);
  ctx.restore();
};
