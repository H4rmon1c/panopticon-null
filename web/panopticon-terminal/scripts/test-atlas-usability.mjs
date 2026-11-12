import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
const root = new URL('../', import.meta.url);
const load = (path) => readFileSync(new URL(path, root), 'utf8');

test('recorded votes prevent contradictory missing-vote and missing-participant claims', () => {
  const context = vm.createContext({});
  vm.runInContext(load('scripts/16-civic-context.js'), context);
  const person = { id: 'voter', type: 'RECORDED_VOTER', name: 'A voter' };
  const vote = { attributes: [], relationships: [{ type: 'RECORDED_VOTE', target_entity_id: 'voter' }], timeline: [] };
  const unknowns = context.civicUnknowns(vote, { byId: new Map([['voter', person]]) });
  assert.ok(!unknowns.some((x) => /No named participant|No vote/.test(x)));
  const empty = context.civicUnknowns({ attributes: [], relationships: [], timeline: [] }, { byId: new Map() });
  assert.ok(empty.some((x) => /No named participant/.test(x)));
  assert.ok(empty.some((x) => /No vote/.test(x)));
});

test('relationship rows do not invent zero percent confidence', () => {
  const list = { children: [], replaceChildren() { this.children = []; }, append(node) { this.children.push(node); } };
  const count = {};
  const context = vm.createContext({
    state: { byId: new Map([['voter', { name: 'A voter' }]]) },
    $: (query) => query === '#relation-list' ? list : count,
    document: { createElement: () => ({ addEventListener() {} }) },
    escapeHtml: String,
  });
  vm.runInContext(load('scripts/03-dossier.js'), context);
  context.renderRelations({ relationships: [{ type: 'RECORDED_VOTE', target_entity_id: 'voter', label: 'Recorded Aye', review_state: 'SOURCE_VERIFIED' }] });
  assert.match(list.children[0].innerHTML, /SOURCE VERIFIED/);
  assert.doesNotMatch(list.children[0].innerHTML, /0%/);
});

test('initial deep link hydrates a decision beyond the first 100 records', async () => {
  const entities = Array.from({ length: 101 }, (_, i) => ({ id: `item-${i}`, type: 'DECISION' }));
  const requested = 'item-100';
  let selected = null;
  const noop = () => {};
  const context = vm.createContext({
    URLSearchParams, location: { hash: `#entity=${requested}`, search: '?ci=1' },
    state: {}, globe: null, Globe: function() { this.setData = noop; },
    $: () => ({}), wireUi: noop,
    client: { bootstrap: async () => ({ mode: 'public', status: {}, featured: [] }),
      search: async () => entities.slice(0, 100),
      getEntitiesByIds: async (ids) => ids.map((id) => entities.find((e) => e.id === id)).filter(Boolean) },
    normalizeEntities: (items) => items,
    renderStatus: noop, renderLayers: noop, renderActivity: noop, renderTimeline: noop, renderCommands: noop,
    selectEntity: async (id) => { selected = id; },
  });
  vm.runInContext(load('scripts/01-boot.js'), context);
  // Use a stub rather than wiring the real browser controls.
  context.wireUi = noop;
  context.normalizeEntities = (items) => items;
  await context.boot();
  assert.equal(selected, requested);
});

test('decision list selects a record and reveals its dossier on narrow screens', () => {
  function node() { return { children: [], attrs: {}, append(...xs) { this.children.push(...xs); }, replaceChildren() { this.children = []; }, setAttribute(k,v) { this.attrs[k] = v; }, addEventListener(_, fn) { this.click = fn; }, querySelectorAll() { return this.children; } }; }
  const list = node(); let selected; let scrolled = false;
  const entries = [{ id: 'place', type: 'JURISDICTION', name: 'Castle Rock' }, { id: 'decision', type: 'DECISION', name: 'A decision', jurisdiction_ids: ['place'], subtitle: 'Pass' }];
  const context = vm.createContext({ client: { snapshot: { entities: entries } },
    currentState: () => ({ selected: { id: 'other' } }),
    civic$: (selector) => selector === '#atlas-record-list' ? list : { scrollIntoView() { scrolled = true; } },
    document: { createElement: node }, selectRecord: (id) => { selected = id; }, matchMedia: () => ({ matches: true }) });
  const source = load('scripts/22-civic-runtime.js');
  vm.runInContext(source.slice(source.indexOf('function renderRecordList()')), context);
  context.renderRecordList();
  assert.equal(list.children.length, 1);
  assert.equal(list.children[0].children[1].textContent, 'Castle Rock · Pass');
  list.children[0].click();
  assert.equal(selected, 'decision');
  assert.equal(list.children[0].attrs['aria-pressed'], 'true');
  assert.equal(scrolled, true);
});

test('changes shortcut opens and closes the activity panel', () => {
  const body = { dataset: {} }; const notices = [];
  const context = vm.createContext({ document: { body }, currentState: () => ({}),
    civic$: () => ({ classList: { add() {}, remove() {} } }), setTimeout: () => {}, announce: (message) => notices.push(message) });
  vm.runInContext(load('scripts/15-civic-actions.js'), context);
  context.runCivicAction('changes'); assert.equal(body.dataset.activity, 'visible');
  context.runCivicAction('changes'); assert.equal(body.dataset.activity, 'hidden');
  assert.deepEqual(notices, ['SHOWING RECENT PUBLISHED CHANGES', 'RECENT CHANGES CLOSED']);
});

test('connections shortcut exposes the map and updates its return control', () => {
  const body = { dataset: {} }; const toggle = { setAttribute(k,v) { this[k] = v; } }; let resized = false;
  const context = vm.createContext({ document: { body }, civic$: () => toggle, atlas: { resize() { resized = true; } } });
  const source = load('scripts/22-civic-runtime.js');
  vm.runInContext(source.slice(source.indexOf('function setAtlasView('), source.indexOf('function renderRecordList()')), context);
  context.setAtlasView(true); assert.equal(body.dataset.atlasView, 'map');
  assert.equal(toggle['aria-pressed'], 'true'); assert.equal(toggle.textContent, 'SHOW DECISION LIST'); assert.equal(resized, true);
  context.setAtlasView(false); assert.equal(body.dataset.atlasView, 'records');
});
