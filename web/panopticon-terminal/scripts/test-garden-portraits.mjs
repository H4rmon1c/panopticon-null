import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { gardenPortraits, gardenPortraitFor, validateGardenPortraitProof } from '../garden-portraits-api.js';
import { filterGardenPortraits } from './filter-portraits.mjs';

const proof = JSON.parse(await readFile(new URL('../../../fixtures/atlas/portrait-sources/garden-city-2026.json', import.meta.url)));
const motion = JSON.parse(await readFile(new URL('./fixtures/garden-motion-valid.json', import.meta.url)));
const research = { records: [{ id: motion.research_record, kind: 'meeting_document', historical: false,
  jurisdiction_ref: motion.jurisdiction_ref, source_url: motion.source.url, source_sha256: motion.source.sha256 }] };

test('page requests the updated motion and validation modules', async () => {
  const html = await readFile(new URL('../garden-motion.html', import.meta.url), 'utf8');
  const script = await readFile(new URL('../garden-motion.js', import.meta.url), 'utf8');
  assert.match(html, /garden-motion\.js\?v=2/);
  assert.match(html, /research\.css\?v=2/);
  assert.match(script, /garden-motion-api\.js\?v=2/);
  assert.match(script, /garden-portraits-api\.js\?v=1/);
});

test('six exact recorded voters bind to official profiles and original image bytes', async () => {
  validateGardenPortraitProof(proof);
  assert.equal(Object.keys(gardenPortraits).length, 6);
  for (const vote of motion.votes) {
    const portrait = gardenPortraitFor(vote);
    assert.ok(portrait);
    assert.match(portrait.profile_url, /^https:\/\/www\.townofgardencity\.com\/[a-z-]+$/);
    assert.equal(gardenPortraitFor({ ...vote, role: 'Other' }), null);
    assert.equal(gardenPortraitFor({ ...vote, position: 'No' }), null);
    assert.equal(gardenPortraitFor({ ...vote, locator: 'extracted line 999' }), null);
    const body = await readFile(new URL('.' + portrait.src.slice(1), new URL('../', import.meta.url)));
    assert.equal(body.length, portrait.image_bytes);
    assert.deepEqual([...body.slice(0, 3)], [0xff, 0xd8, 0xff]);
    assert.equal(createHash('sha256').update(body).digest('hex'), portrait.image_sha256);
  }
  assert.equal(gardenPortraitFor({ name: '__proto__', role: 'Trustee', position: 'Yes' }), null);
});

test('swapped or changed source bindings fail closed', () => {
  for (const field of ['name', 'source_profile', 'profile_sha256', 'image_sha256', 'width', 'height', 'profile_role_at_capture']) {
    const altered = structuredClone(proof); altered.portraits[0][field] = 'changed';
    assert.throws(() => validateGardenPortraitProof(altered));
  }
  const swapped = structuredClone(proof); swapped.portraits[0].image_sha256 = swapped.portraits[1].image_sha256;
  assert.throws(() => validateGardenPortraitProof(swapped));
});

test('release filter removes all photos when motion is unavailable', async () => {
  const stage = await mkdtemp(join(tmpdir(), 'garden-photo-filter-'));
  try {
    await mkdir(join(stage, 'garden-portraits'));
    for (const item of Object.values(gardenPortraits)) await writeFile(join(stage, 'garden-portraits', item.image_sha256 + '.jpg'), 'fixture');
    await filterGardenPortraits(stage, motion, research);
    assert.equal((await readdir(join(stage, 'garden-portraits'))).length, 6);
    await assert.rejects(filterGardenPortraits(stage, motion, { records: [] }));
    await filterGardenPortraits(stage, { schema_version: motion.schema_version, state: 'not_available' }, { records: [] });
    assert.deepEqual(await readdir(join(stage, 'garden-portraits')), []);
  } finally { await rm(stage, { recursive: true, force: true }); }
});

test('motion view renders six attributed photos only after source binding', async () => {
  class Element {
    constructor(tag) { this.tagName = tag; this.children = []; this.textContent = ''; this.className = ''; }
    append(...items) { this.children.push(...items); }
    addEventListener() {}
    remove() { this.removed = true; }
  }
  const status = new Element('status'); const content = new Element('content');
  const oldDocument = globalThis.document; const oldFetch = globalThis.fetch;
  try {
    globalThis.document = { querySelector: (selector) => selector === '#motion-status' ? status : content,
      createElement: (tag) => new Element(tag) };
    const actualResearch = await readFile(new URL('../public/research-signals.json', import.meta.url));
    globalThis.fetch = async (url) => new Response(url.includes('garden-city-motion') ? JSON.stringify(motion) : actualResearch, { status: 200 });
    await import('../garden-motion.js?portrait-test=available');
    const walk = (element) => [element, ...element.children.flatMap(walk)];
    const nodes = content.children.flatMap(walk);
    const photos = nodes.filter((node) => node.tagName === 'img');
    assert.equal(photos.length, 6);
    for (const vote of motion.votes) {
      const image = photos.find((photo) => photo.alt === `Official profile photo of ${vote.name}`);
      assert.equal(image?.src, gardenPortraitFor(vote).src);
      assert.ok(nodes.some((node) => node.tagName === 'a' && node.href === gardenPortraitFor(vote).profile_url));
    }
    assert.match(status.textContent, /Source-bound/);
    assert.ok(nodes.some((node) => node.textContent.includes('When the photos were taken is not established')));
    const mismatch = new Element('content');
    globalThis.document.querySelector = (selector) => selector === '#motion-status' ? status : mismatch;
    const alteredResearch = JSON.parse(actualResearch.toString());
    alteredResearch.records = alteredResearch.records.filter((record) => record.id !== motion.research_record);
    globalThis.fetch = async (url) => new Response(url.includes('garden-city-motion') ? JSON.stringify(motion) : JSON.stringify(alteredResearch), { status: 200 });
    await import('../garden-motion.js?portrait-test=unbound');
    assert.equal(mismatch.children.length, 0);
    assert.match(status.textContent, /unavailable/);
    const hidden = new Element('content');
    globalThis.document.querySelector = (selector) => selector === '#motion-status' ? status : hidden;
    globalThis.fetch = async () => new Response(JSON.stringify({ schema_version: motion.schema_version, state: 'not_available' }), { status: 200 });
    await import('../garden-motion.js?portrait-test=unavailable');
    assert.equal(hidden.children.length, 0);
    assert.match(status.textContent, /unavailable/);
  } finally { globalThis.document = oldDocument; globalThis.fetch = oldFetch; }
});
