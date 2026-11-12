import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { validateCortezMotion, validateCortezResearchBinding } from '../cortez-motion-api.js';
const valid = JSON.parse(readFileSync(new URL('../public/cortez-motion-v1.json', import.meta.url)));
const research = JSON.parse(readFileSync(new URL('../public/research-signals.json', import.meta.url)));
test('source-bound final-reading record validates', () => {
  assert.equal(validateCortezMotion(valid).votes.length, 7);
  validateCortezResearchBinding(valid, research);
  assert.equal(validateCortezMotion({ schema_version: valid.schema_version, state: 'not_available' }).state, 'not_available');
});
test('changed votes, scope, identity, locators or URLs fail closed', () => {
  const attacks = [d => d.votes[0].name = 'Other', d => d.votes[1].role = 'Mayor',
    d => d.votes[2].position = 'No', d => d.votes.reverse(), d => d.scope = 'Consent vote',
    d => d.source.vote_page = 1, d => d.source.roster_page = 2, d => d.source.sha256 = '0'.repeat(64),
    d => d.source.url = 'https://evil.example/', d => d.source.index_url += '&token=fake',
    d => d.source.receipt_sha256 = 'missing', d => d.notice = '<img src=x>', d => d.extra = 'unsafe'];
  for (const attack of attacks) { const copy = structuredClone(valid); attack(copy); assert.throws(() => validateCortezMotion(copy)); }
});
test('motion and current research source must bind uniquely', () => {
  const record = research.records.find(r => r.id === valid.research_record);
  for (const rows of [[], [record, record], ...['id','kind','historical','jurisdiction_ref','source_url','source_sha256'].map(k => [{...record,[k]:'changed'}])])
    assert.throws(() => validateCortezResearchBinding(valid,{records:rows}));
});
test('browser displays seven votes only when sidecar and research both match', async () => {
  class Element {
    constructor(tag) { this.tag = tag; this.children = []; this.textContent = ''; }
    append(...children) { this.children.push(...children); }
    addEventListener(event,handler) { this[event] = handler; }
    remove() { this.removed = true; }
  }
  async function render(sidecar, report, version) {
    const status = new Element('status'), content = new Element('content');
    globalThis.document = {querySelector: s => s === '#motion-status' ? status : content, createElement: tag => new Element(tag)};
    globalThis.fetch = async url => new Response(JSON.stringify(url.includes('cortez-motion') ? sidecar : report));
    await import(`../cortez-motion.js?test=${version}`);
    return {status,content};
  }
  const ok = await render(valid,research,'valid');
  assert.match(ok.status.textContent,/preliminary/);
  assert.equal(ok.content.children.find(x => x.tag === 'ul').children.length,7);
  assert.ok(ok.content.children.some(x => x.textContent.includes('When available, Lucero’s portrait') && x.textContent.includes('unknown')));
  const voters = ok.content.children.find(x => x.tag === 'ul').children;
  const images = voters.flatMap(v => v.children.filter(child => child.tag === 'img'));
  assert.equal(images.length, 1);
  assert.match(images[0].alt, /Orly Lucero/);
  images[0].error(); assert.equal(images[0].removed, true);
  const missing = await render({schema_version:valid.schema_version,state:'not_available'},research,'missing');
  assert.match(missing.status.textContent,/unavailable/); assert.equal(missing.content.children.length,0);
  const altered = structuredClone(research); altered.records.find(r => r.id === valid.research_record).historical = true;
  const withdrawn = await render(valid,altered,'withdrawn');
  assert.match(withdrawn.status.textContent,/unavailable/); assert.equal(withdrawn.content.children.length,0);
});
