class Globe {
  constructor(canvas, { onSelect, onCamera }) {
    this.canvas = canvas;
    this.ctx = canvas.getContext("2d");
    this.onSelect = onSelect;
    this.onCamera = onCamera;
    this.entities = [];
    this.byId = new Map();
    this.links = [];
    this.layers = new Set(Object.keys(TYPE_GROUPS));
    this.selected = null;
    this.view = "global";
    this.sensor = "record";
    this.timeline = 1;
    this.lon = -105; this.lat = 38; this.zoom = 1.04;
    this.targetLon = this.lon; this.targetLat = this.lat; this.targetZoom = this.zoom;
    this.hits = []; this.pings = new Map(); this.drag = null;
    this.stars = seeded(360, 91823).map((n, i, a) => ({ x:n, y:a[(i+1)%a.length], s:.45+a[(i+2)%a.length], a:.2+a[(i+3)%a.length]*.8 }));
    this.last = performance.now(); this.lastInteraction = this.last;
    this.ciMode = new URLSearchParams(location.search).has("ci"); this.frameCount = 0;
    this.resize = this.resize.bind(this); this.frame = this.frame.bind(this);
    window.addEventListener("resize", this.resize); this.wire(); this.resize(); requestAnimationFrame(this.frame);
  }
  setData(entities) { this.entities = entities.filter((entity) => validGeo(entity.geo)); this.byId = new Map(this.entities.map((entity) => [entity.id,entity])); this.links = uniqueLinks(this.entities); if (this.ciMode) this.draw(performance.now()); }
  setLayers(layers) { this.layers = new Set(layers); if (this.ciMode) this.draw(performance.now()); }
  setView(view) { this.view = view; if (this.ciMode) this.draw(performance.now()); }
  setSensor(sensor) { this.sensor = sensor; if (this.ciMode) this.draw(performance.now()); }
  setTimeline(value) { this.timeline = value; if (this.ciMode) this.draw(performance.now()); }
  select(id) { this.selected = id; this.ping(id); if (this.ciMode) this.draw(performance.now()); }
  ping(id) { this.pings.set(id, performance.now()); }
  resize() { const rect = this.canvas.getBoundingClientRect(); const dpr = Math.min(devicePixelRatio || 1, 2); this.canvas.width = Math.max(1, Math.round(rect.width*dpr)); this.canvas.height = Math.max(1, Math.round(rect.height*dpr)); this.ctx.setTransform(dpr,0,0,dpr,0,0); this.width=rect.width; this.height=rect.height; if (this.ciMode) this.draw(performance.now()); }
  reset() { this.targetLon=-105; this.targetLat=38; this.targetZoom=1.04; }
  focus(geo, zoom=1.2) { this.targetLon=Number(geo.lon); this.targetLat=clamp(Number(geo.lat),-75,75); this.targetZoom=zoom; this.lastInteraction=performance.now(); }
  focusNetwork(id) {
    const entity = this.byId.get(id); const ids = new Set([id]);
    entity?.relationships.forEach((link) => ids.add(link.target_entity_id));
    this.links.forEach((link) => { if (link.target_entity_id===id) ids.add(link.source_entity_id); });
    const points=[...ids].map((key)=>this.byId.get(key)?.geo).filter(Boolean); if (!points.length) return this.reset();
    this.targetLon=circularMean(points.map((point)=>point.lon)); this.targetLat=points.reduce((s,p)=>s+Number(p.lat),0)/points.length; this.targetZoom=points.length<3?1.48:1.18; this.lastInteraction=performance.now();
  }
  frame(now) {
    const dt=Math.min(50,now-this.last); this.last=now; const settle=1-Math.pow(.001,dt/1000);
    this.lon=lerpAngle(this.lon,this.targetLon,settle); this.lat+=(this.targetLat-this.lat)*settle; this.zoom+=(this.targetZoom-this.zoom)*settle;
    if (this.view==="global" && now-this.lastInteraction>9000 && !this.drag) this.targetLon += dt*.000018;
    this.draw(now); this.onCamera?.({lat:this.lat,lon:normalizeLon(this.lon),altitude:7600/Math.max(.7,this.zoom)}); this.frameCount += 1;
    if (!this.ciMode || this.frameCount < 3) requestAnimationFrame(this.frame);
  }
}
