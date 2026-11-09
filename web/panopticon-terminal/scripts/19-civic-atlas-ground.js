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

  ctx.fillStyle = "rgba(139, 172, 184, .55)";
  ctx.font = "700 8px ui-monospace, monospace";
  ctx.fillText("SCHEMATIC RELATIONSHIPS · JURISDICTIONAL ANCHORS ONLY", 18, this.height - 18);

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
