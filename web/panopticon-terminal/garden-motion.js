import { validateGardenMotion, validateGardenResearchBinding } from './garden-motion-api.js?v=2';
import { readBoundedResearchResponse } from './research-api.js?v=garden-motion-1';
import { gardenPortraitFor } from './garden-portraits-api.js?v=1';

const status = document.querySelector('#motion-status');
const content = document.querySelector('#motion-content');
const node = (tag, text, cls = '') => { const el = document.createElement(tag); el.textContent = text; if (cls) el.className = cls; return el; };
const link = (url, label) => { const el = node('a', label, 'source'); el.href = url; el.target = '_blank'; el.rel = 'noopener noreferrer'; return el; };
try {
  const response = await fetch('./public/garden-city-motion-v1.json', { cache: 'no-store' });
  if (!response.ok || !response.body || Number(response.headers.get('content-length') ?? 0) > 65536) throw new Error('unavailable');
  const reader = response.body.getReader(); const chunks = []; let size = 0;
  try { while (true) { const { value, done } = await reader.read(); if (done) break; size += value.byteLength; if (size > 65536) throw new Error('too large'); chunks.push(value); } }
  finally { await reader.cancel().catch(() => {}); }
  const bytes = new Uint8Array(size); let offset = 0; for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
  const data = validateGardenMotion(JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes)));
  if (data.state !== 'available') throw new Error('unavailable');
  const research = await readBoundedResearchResponse(await fetch('./public/research-signals.json', { cache: 'no-store' }));
  validateGardenResearchBinding(data, research);
  status.textContent = 'Source-bound preliminary motion; not a released decision record.';
  content.append(node('p', `${data.jurisdiction} · ${data.body} · ${data.meeting_date}`, 'meta'));
  content.append(node('h2', data.motion)); content.append(node('p', data.outcome, 'title'));
  content.append(node('p', data.notice, 'note')); content.append(node('h3', 'Recorded votes on this motion'));
  content.append(node('p', 'Photos are from town profiles captured in November 2026. When the photos were taken is not established; the vote is recorded separately in the minutes.', 'meta'));
  const list = node('ul', '', 'garden-votes');
  for (const vote of data.votes) {
    const item = node('li', '', 'garden-vote');
    const portrait = gardenPortraitFor(vote);
    if (portrait) {
      const photo = document.createElement('img');
      photo.src = portrait.src; photo.alt = `Official profile photo of ${vote.name}`;
      photo.width = 96; photo.height = 120; photo.loading = 'lazy'; photo.decoding = 'async';
      photo.className = 'garden-vote__photo';
      photo.addEventListener('error', () => photo.remove());
      item.append(photo);
    }
    const info = node('div', '', 'garden-vote__details');
    info.append(node('strong', vote.name), node('p', `${vote.role} · ${vote.position}`, 'meta'));
    if (portrait) info.append(link(portrait.profile_url, 'Town profile and photo ↗'));
    item.append(info); list.append(item);
  }
  content.append(list);
  const photoProof = document.createElement('details'); photoProof.className = 'proof';
  photoProof.append(node('summary', 'Portrait source digests'));
  for (const vote of data.votes) {
    const portrait = gardenPortraitFor(vote);
    if (portrait) photoProof.append(node('p', `${vote.name}: photo SHA-256 ${portrait.image_sha256}; profile SHA-256 ${portrait.profile_sha256}`, 'digest'));
  }
  content.append(photoProof);
  content.append(node('p', `${data.attendance.name} · ${data.attendance.role} · ${data.attendance.status}. This is attendance, not a recorded vote.`, 'meta'));
  content.append(link(`${data.source.url}#page=${data.source.motion_page}`, 'Open the motion and votes in the later packet, page 6 ↗'));
  content.append(link(`${data.source.url}#page=${data.source.meeting_page}`, 'Open the meeting and attendance record, page 3 ↗'));
  content.append(link(data.source.index_url, 'Open the town’s board-meeting index ↗'));
  content.append(node('h3', 'Later approval of the minutes'));
  if (data.minutes_adoption === null) {
    content.append(node('p', 'The source for later adoption of these minutes is unavailable or unverified.', 'note'));
  } else {
    const adoption = data.minutes_adoption;
    content.append(node('p', `${adoption.date}: ${adoption.scope}. ${adoption.outcome} (${adoption.yes_count} yes).`, 'meta'));
    content.append(node('p', adoption.notice, 'note'));
    content.append(link(`${adoption.source.url}#page=${adoption.source.meeting_page}`, 'Open the consent item and motion, page 1 ↗'));
    content.append(link(`${adoption.source.url}#page=${adoption.source.vote_page}`, 'Open the consent vote, page 2 ↗'));
    const proof = document.createElement('details'); proof.className = 'proof'; proof.append(node('summary', 'Adoption source capture details'));
    for (const text of [`Captured ${adoption.source.captured_at}`, `Consent item, motion, and vote: ${adoption.source.locator}`,
      `PDF SHA-256 ${adoption.source.sha256}`, `Extracted text SHA-256 ${adoption.source.text_sha256}`,
      `Town index anchor ${adoption.source.index_anchor}; SHA-256 ${adoption.source.index_sha256}`,
      `Archived PDF receipt SHA-256 ${adoption.source.receipt_sha256}`, `Archived index receipt SHA-256 ${adoption.source.index_receipt_sha256}`]) proof.append(node('p', text, 'digest'));
    content.append(proof);
  }
  const details = document.createElement('details'); details.className = 'proof'; details.append(node('summary', 'Source capture details'));
  for (const text of [`Captured ${data.source.captured_at}`, `Meeting: ${data.source.meeting_locator}; attendance: ${data.attendance.locator}; motion and roll call: ${data.source.motion_locator}.`,
    `PDF SHA-256 ${data.source.sha256}`, `Extracted text SHA-256 ${data.source.text_sha256}`,
    `Town index anchor ${data.source.index_anchor}; SHA-256 ${data.source.index_sha256}`,
    `Archived PDF receipt SHA-256 ${data.source.receipt_sha256}`, `Archived index receipt SHA-256 ${data.source.index_receipt_sha256}`]) details.append(node('p', text, 'digest'));
  content.append(details);
} catch { status.textContent = 'This preliminary motion record is currently unavailable.'; }
