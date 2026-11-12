import { validateGeorgetownMotion, validateGeorgetownResearchBinding } from './georgetown-motion-api.js?v=1';
import { readBoundedResearchResponse } from './research-api.js?v=garden-motion-1';
const status = document.querySelector('#motion-status'); const content = document.querySelector('#motion-content');
const node = (tag, text, cls = '') => { const el = document.createElement(tag); el.textContent = text; if (cls) el.className = cls; return el; };
const link = (url, label) => { const el = node('a', label, 'source'); el.href = url; el.target = '_blank'; el.rel = 'noopener noreferrer'; return el; };
try {
  const response = await fetch('./public/georgetown-motion-v1.json', { cache: 'no-store' });
  if (!response.ok || !response.body || Number(response.headers.get('content-length') ?? 0) > 65536) throw new Error('unavailable');
  const reader = response.body.getReader(); let size = 0; const chunks = [];
  try { while (true) { const { value, done } = await reader.read(); if (done) break; size += value.byteLength; if (size > 65536) throw new Error('too large'); chunks.push(value); } }
  finally { await reader.cancel().catch(() => {}); }
  const bytes = new Uint8Array(size); let at = 0; for (const chunk of chunks) { bytes.set(chunk, at); at += chunk.byteLength; }
  const data = validateGeorgetownMotion(JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes)));
  if (data.state !== 'available') throw new Error('unavailable');
  const research = await readBoundedResearchResponse(await fetch('./public/research-signals.json', { cache: 'no-store' }));
  validateGeorgetownResearchBinding(data, research);
  status.textContent = 'Source-bound preliminary motion; not a released decision record.';
  content.append(node('p', `${data.jurisdiction} · ${data.body} · ${data.meeting_date}`, 'meta'));
  content.append(node('h2', data.motion), node('p', data.outcome, 'title'), node('p', data.motion_details, 'meta'), node('p', data.notice, 'note'));
  content.append(node('h3', 'Recorded votes on the tabling motion'));
  const list = node('ul', ''); for (const vote of data.votes) list.append(node('li', `${vote.name} · ${vote.role} · ${vote.position}`)); content.append(list);
  content.append(node('p', 'Portraits have not been established for these officials.', 'meta'));
  content.append(link(`${data.source.url}#page=${data.source.motion_page}`, 'Open the tabling motion and tie-break, page 5 ↗'));
  content.append(link(`${data.source.url}#page=${data.source.roster_page}`, 'Open the meeting roll call, page 2 ↗'));
  content.append(link(data.source.index_url, 'Open the town’s meeting index ↗'));
  const proof = document.createElement('details'); proof.className = 'proof'; proof.append(node('summary', 'Preserved source capture details'));
  for (const t of [`Captured ${data.source.captured_at}`, `PDF SHA-256 ${data.source.sha256}`, `OCR text SHA-256 ${data.source.text_sha256}`,
    `Town index anchor ${data.source.index_anchor}; SHA-256 ${data.source.index_sha256}`, `Indexed link: ${data.source.indexed_url}`,
    `Timestamped vendor document: ${data.source.indexed_document_url}`, `The timestamped vendor URL was captured separately and produced the same PDF digest; both redirects were retained.`,
    `Document receipt SHA-256 ${data.source.receipt_sha256}`, `Index receipt SHA-256 ${data.source.index_receipt_sha256}`,
    `Timestamped document receipt SHA-256 ${data.source.indexed_document_receipt_sha256}`, `Indexed redirect receipt SHA-256 ${data.source.indexed_redirect_receipt_sha256}`, `Adjusted redirect receipt SHA-256 ${data.source.redirect_receipt_sha256}`]) proof.append(node('p', t, 'digest'));
  content.append(proof);
} catch { status.textContent = 'This preliminary motion record is currently unavailable.'; }
