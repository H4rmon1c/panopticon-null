CivicAtlas.prototype.drawGround = function drawGround(ctx, now) {
  const gradient = ctx.createLinearGradient(0, 0, this.width, this.height);
  gradient.addColorStop(0, "#0b1b27");
  gradient.addColorStop(0.54, "#07121b");
  gradient.addColorStop(1, "#091722");
  ctx.fillStyle = gradient;
  ctx.fillRect(0, 0, this.width, this.height);

  const minor = 24 * this.zoom;
  const major = minor * 4;
  ctx.lineWidth = 1;
  ctx.strokeStyle = "rgba(122, 181, 204, .045)";
  for (let x = ((this.pan.x % minor) + minor) % minor; x < this.width; x += minor) {
    ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, this.height); ctx.stroke();
  }
  for (let y = ((this.pan.y % minor) + minor) % minor; y < this.height; y += minor) {
    ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(this.width, y); ctx.stroke();
  }
  ctx.strokeStyle = "rgba(122, 181, 204, .075)";
  for (let x = ((this.pan.x % major) + major) % major; x < this.width; x += major) {
    ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, this.height); ctx.stroke();
  }
  for (let y = ((this.pan.y % major) + major) % major; y < this.height; y += major) {
    ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(this.width, y); ctx.stroke();
  }

  const boundary = [[0.11,0.23],[0.29,0.11],[0.55,0.16],[0.80,0.10],[0.91,0.29],[0.86,0.58],[0.94,0.82],[0.70,0.91],[0.44,0.87],[0.18,0.94],[0.08,0.67]];
  ctx.save();
  ctx.setLineDash([8, 7]);
  ctx.strokeStyle = "rgba(111, 225, 241, .18)";
  ctx.lineWidth = 1;
  ctx.beginPath();
  boundary.forEach(([x, y], index) => {
    const p = this.point(x, y);
    index ? ctx.lineTo(p.x, p.y) : ctx.moveTo(p.x, p.y);
  });
  ctx.closePath();
  ctx.stroke();
  ctx.restore();

  const roads = [
    [[0.05,0.36],[0.24,0.34],[0.43,0.41],[0.62,0.37],[0.94,0.42]],
    [[0.12,0.78],[0.26,0.61],[0.49,0.56],[0.67,0.45],[0.88,0.24]],
    [[0.34,0.05],[0.37,0.29],[0.43,0.51],[0.41,0.92]],
    [[0.71,0.07],[0.69,0.29],[0.74,0.52],[0.67,0.92]],
    [[0.05,0.64],[0.29,0.70],[0.53,0.67],[0.94,0.73]],
  ];
  roads.forEach((road, index) => {
    ctx.beginPath();
    road.forEach(([x, y], pointIndex) => {
      const p = this.point(x, y);
      pointIndex ? ctx.lineTo(p.x, p.y) : ctx.moveTo(p.x, p.y);
    });
    ctx.strokeStyle = index < 2 ? "rgba(188, 211, 220, .10)" : "rgba(188, 211, 220, .065)";
    ctx.lineWidth = index < 2 ? 2 : 1;
    ctx.stroke();
  });

  const blocks = [
    [0.16,0.18,0.16,0.13,"PUBLIC BODY"], [0.58,0.16,0.20,0.12,"OWNERSHIP"],
    [0.12,0.55,0.18,0.17,"UTILITY"], [0.41,0.38,0.22,0.18,"SELECTED PLACE"],
    [0.68,0.52,0.20,0.16,"BUILD CHAIN"], [0.35,0.72,0.22,0.13,"MONEY / TERMS"],
  ];
  blocks.forEach(([x, y, w, h, label]) => {
    const a = this.point(x, y);
    const b = this.point(x + w, y + h);
    ctx.fillStyle = "rgba(99, 153, 173, .025)";
    ctx.strokeStyle = "rgba(132, 187, 207, .08)";
    ctx.fillRect(a.x, a.y, b.x - a.x, b.y - a.y);
    ctx.strokeRect(a.x, a.y, b.x - a.x, b.y - a.y);
    ctx.fillStyle = "rgba(139, 172, 184, .24)";
    ctx.font = "700 7px ui-monospace, monospace";
    ctx.fillText(label, a.x + 7, a.y + 13);
  });

  if (!this.reducedMotion) {
    const scanX = ((now * 0.015) % (this.width + 160)) - 80;
    const scan = ctx.createLinearGradient(scanX - 60, 0, scanX + 60, 0);
    scan.addColorStop(0, "rgba(111,225,241,0)");
    scan.addColorStop(0.5, "rgba(111,225,241,.035)");
    scan.addColorStop(1, "rgba(111,225,241,0)");
    ctx.fillStyle = scan;
    ctx.fillRect(scanX - 60, 0, 120, this.height);
  }
};
