function CivicAtlas(canvas, { onSelect } = {}) {
  this.canvas = canvas;
  this.ctx = canvas.getContext("2d", { alpha: false });
  this.onSelect = onSelect;
  this.entities = [];
  this.byId = new Map();
  this.activeLayers = new Set();
  this.selected = null;
  this.view = "place";
  this.nodes = [];
  this.hits = [];
  this.width = 1;
  this.height = 1;
  this.dpr = 1;
  this.pan = { x: 0, y: 0 };
  this.zoom = 1;
  this.drag = null;
  this.moved = false;
  this.reducedMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;
  this.resize = this.resize.bind(this);
  this.frame = this.frame.bind(this);
  this.wire();
  this.resize();
  requestAnimationFrame(this.frame);
}
CivicAtlas.prototype.wire = function wire() {
  addEventListener("resize", this.resize);
  this.canvas.addEventListener("pointerdown", (event) => {
    this.drag = { x: event.clientX, y: event.clientY, panX: this.pan.x, panY: this.pan.y };
    this.moved = false;
    this.canvas.setPointerCapture?.(event.pointerId);
  });
  this.canvas.addEventListener("pointermove", (event) => {
    if (!this.drag) return;
    const dx = event.clientX - this.drag.x;
    const dy = event.clientY - this.drag.y;
    if (Math.abs(dx) + Math.abs(dy) > 4) this.moved = true;
    this.pan.x = this.drag.panX + dx;
    this.pan.y = this.drag.panY + dy;
  });
  this.canvas.addEventListener("pointerup", (event) => {
    const rect = this.canvas.getBoundingClientRect();
    const point = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    const hit = !this.moved && [...this.hits].reverse().find((item) => point.x >= item.x && point.x <= item.x + item.w && point.y >= item.y && point.y <= item.y + item.h);
    this.drag = null;
    if (hit?.id) this.onSelect?.(hit.id);
  });
  this.canvas.addEventListener("wheel", (event) => {
    event.preventDefault();
    this.zoom = clampCivic(this.zoom * (event.deltaY > 0 ? 0.92 : 1.08), 0.72, 1.52);
  }, { passive: false });
  this.canvas.addEventListener("dblclick", () => {
    this.pan = { x: 0, y: 0 };
    this.zoom = 1;
  });
};

CivicAtlas.prototype.setData = function setData(entities) {
  this.entities = entities ?? [];
  this.byId = new Map(this.entities.map((entity) => [entity.id, entity]));
  if (!this.selected && this.entities[0]) this.selected = this.entities[0].id;
  this.layout();
};

CivicAtlas.prototype.setLayers = function setLayers(layers) {
  this.activeLayers = new Set(layers ?? []);
  this.layout();
};

CivicAtlas.prototype.setSelected = function setSelected(id) {
  if (!id) return;
  this.selected = id;
  this.layout();
};

CivicAtlas.prototype.setView = function setView(view) {
  this.view = view === "connections" ? "connections" : "place";
  this.layout();
};

CivicAtlas.prototype.resize = function resize() {
  const rect = this.canvas.getBoundingClientRect();
  this.dpr = Math.min(devicePixelRatio || 1, 2);
  this.width = Math.max(1, rect.width);
  this.height = Math.max(1, rect.height);
  this.canvas.width = Math.round(this.width * this.dpr);
  this.canvas.height = Math.round(this.height * this.dpr);
  this.ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
  this.layout();
};

CivicAtlas.prototype.visible = function visible(entity) {
  if (!this.activeLayers.size || typeof TYPE_GROUPS === "undefined") return true;
  const type = String(entity.type ?? "").toUpperCase();
  return [...this.activeLayers].some((layer) => TYPE_GROUPS[layer]?.has(type));
};
