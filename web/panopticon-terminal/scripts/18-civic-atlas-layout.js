CivicAtlas.prototype.layout = function layout() {
  const visible = this.entities.filter((entity) => this.visible(entity));
  const selected = this.byId.get(this.selected) ?? visible[0];
  const relations = new Map((selected?.relationships ?? []).map((item) => [item.target_entity_id, item]));
  const basePositions = {
    FACILITY: [0.52, 0.48], DATACENTER: [0.52, 0.48], CAMPUS: [0.52, 0.48], PROJECT: [0.52, 0.48],
    ORGANIZATION: [0.22, 0.25], COMPANY: [0.22, 0.25], AGENCY: [0.22, 0.25], OPERATOR: [0.22, 0.25],
    UTILITY: [0.20, 0.69], POWER: [0.20, 0.69], SUBSTATION: [0.20, 0.69],
    NETWORK: [0.80, 0.28], CARRIER: [0.80, 0.28], FIBER: [0.80, 0.28],
    CONTRACTOR: [0.78, 0.65], SUPPLIER: [0.67, 0.79], VENDOR: [0.78, 0.65],
    CONTRACT: [0.46, 0.82], AGREEMENT: [0.46, 0.82], AWARD: [0.46, 0.82], SOLICITATION: [0.46, 0.82],
    DECISION: [0.36, 0.18], VOTE: [0.36, 0.18], ORDINANCE: [0.36, 0.18], PERMIT: [0.36, 0.18],
    PERSON: [0.66, 0.17], OFFICIAL: [0.66, 0.17], OFFICEHOLDER: [0.66, 0.17],
  };
  const groupCounts = new Map();
  this.nodes = visible.map((entity, index) => {
    const type = String(entity.type ?? "ENTITY").toUpperCase();
    let nx;
    let ny;
    if (this.view === "connections" && selected) {
      if (entity.id === selected.id) {
        nx = 0.5; ny = 0.48;
      } else if (relations.has(entity.id)) {
        const linked = [...relations.keys()];
        const position = linked.indexOf(entity.id);
        const angle = -Math.PI / 2 + (position / Math.max(1, linked.length)) * Math.PI * 2;
        nx = 0.5 + Math.cos(angle) * 0.30;
        ny = 0.48 + Math.sin(angle) * 0.29;
      } else {
        const angle = (index / Math.max(1, visible.length)) * Math.PI * 2;
        nx = 0.5 + Math.cos(angle) * 0.43;
        ny = 0.48 + Math.sin(angle) * 0.39;
      }
    } else {
      const base = basePositions[type] ?? [0.5, 0.5];
      const count = groupCounts.get(type) ?? 0;
      groupCounts.set(type, count + 1);
      const angle = count * 2.2 + index * 0.37;
      nx = base[0] + Math.cos(angle) * Math.min(0.08, count * 0.025);
      ny = base[1] + Math.sin(angle) * Math.min(0.07, count * 0.022);
      if (entity.id === selected?.id) { nx = 0.52; ny = 0.48; }
    }
    return { entity, nx, ny };
  });
};

CivicAtlas.prototype.frame = function frame(now) {
  this.draw(now);
  requestAnimationFrame(this.frame);
};

CivicAtlas.prototype.draw = function draw(now) {
  const ctx = this.ctx;
  ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
  ctx.clearRect(0, 0, this.width, this.height);
  this.drawGround(ctx, now);
  if (!this.nodes.length) {
    ctx.fillStyle = "#6f8995";
    ctx.font = "700 11px ui-monospace, monospace";
    ctx.fillText("INDEXING PUBLIC RECORDS…", 24, 52);
    return;
  }
  this.drawLinks(ctx, now);
  this.drawNodes(ctx, now);
  this.drawFurniture(ctx);
};

CivicAtlas.prototype.point = function point(nx, ny) {
  return {
    x: (nx - 0.5) * this.width * this.zoom + this.width / 2 + this.pan.x,
    y: (ny - 0.5) * this.height * this.zoom + this.height / 2 + this.pan.y,
  };
};
