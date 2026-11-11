import { readBoundedResearchResponse } from './research-api.js';
import { votePortraits } from './portraits/portraits.js';
import { lookupResearchPortrait } from './portraits/lookup.js';

const summary = document.querySelector('#research-summary');
const search = document.querySelector('#research-search');
const count = document.querySelector('#research-count');
const results = document.querySelector('#research-results');
const selected = new URLSearchParams(location.search).get('jurisdiction');
const jurisdiction = /^us-co-[a-z0-9-]{1,100}$/.test(selected ?? '') ? selected : null;
const node = (tag, name, value) => {
  const element = document.createElement(tag);
  if (name) element.className = name;
  if (value) element.textContent = value;
  return element;
};
const external = (url, text) => {
  const link = node('a', 'source', text);
  link.href = url; link.target = '_blank'; link.rel = 'noopener noreferrer';
  return link;
};

function render(report) {
  const query = search.value.trim().toLocaleLowerCase();
  const matches = report.records.filter((item) =>
    (!jurisdiction || item.jurisdiction_ref === jurisdiction) &&
    `${item.jurisdiction} ${item.file} ${item.title} ${item.signals.join(' ')}`.toLocaleLowerCase().includes(query));
  const latest = (item) => (item.actions ?? []).reduce((date, action) =>
    /^\d{4}-\d{2}-\d{2}$/.test(action.date) && action.date > date ? action.date : date, '');
  matches.sort((left, right) => latest(right).localeCompare(latest(left)) || left.jurisdiction.localeCompare(right.jurisdiction));
  count.textContent = `${matches.length.toLocaleString()} matches${jurisdiction ? ' in this jurisdiction' : ''}${matches.length > 100 ? ' · showing the first 100' : ''}`;
  results.replaceChildren();
  for (const item of matches.slice(0, 100)) {
    const card = node('article', 'card');
    card.append(node('p', 'label', 'PRELIMINARY SOURCE REFERENCE'));
    card.append(node('h2', '', item.jurisdiction));
    if (item.file) card.append(node('p', 'file', item.file));
    card.append(node('p', 'title', item.title || 'A relevant term was found in this source.'));
    card.append(node('p', 'signals', item.signals.join(' · ')));
    if (item.source_status) card.append(node('p', 'meta', `The source lists its status as “${item.source_status}.” This is not an individual roll call.`));
    if (item.historical) card.append(node('p', 'meta', 'Historical capture: a later source check changed this record or no longer found the same reference.'));
    if (item.ocr) card.append(node('p', 'meta', 'Text was read with OCR and may contain errors.'));
    for (const motion of item.actions ?? []) {
      const section = node('section', 'motion');
      section.append(node('h3', '', `Recorded action · ${motion.date.replace('not_established', 'date not established')}`));
      section.append(node('p', 'meta', [motion.body, motion.action, `Outcome: ${motion.outcome.replace('not_established', 'not established')}`].filter(Boolean).join(' · ')));
      if (motion.historical) section.append(node('p', 'meta', 'Historical capture: the matter history changed after this action was captured.'));
      const scopes = {
        item: 'The source attaches these votes to this agenda item.',
        consent_block: 'These votes were recorded on the consent agenda containing this item; a separate item vote is not established.',
        consent_designated_item: 'The source marks this item as consent and attaches these votes to it; their exact scope needs confirmation.',
        not_established: 'Individual votes are not established in this capture.',
      };
      section.append(node('p', 'meta', scopes[motion.vote_scope]));
      if (motion.votes.length) {
        const people = node('div', 'people');
        for (const vote of motion.votes) {
          const entry = node('div', 'person');
          const client = new URL(item.matter_url).pathname.split('/')[2];
          const portrait = vote.portrait ?? lookupResearchPortrait(votePortraits, client, item.jurisdiction_ref, vote);
          if (portrait) {
            const picture = node('img', 'person__photo');
            picture.src = portrait.src; picture.alt = `Official portrait of ${vote.name}`; picture.loading = 'lazy';
            entry.append(picture);
          }
          const detail = node('div', 'person__details');
          detail.append(node('strong', '', vote.name));
          detail.append(node('p', 'meta', `Recorded: ${vote.value}`));
          if (portrait) {
            detail.append(external(portrait.source_url, 'Portrait source ↗'));
            detail.append(node('p', 'meta', 'Portrait date not established.'));
          }
          entry.append(detail); people.append(entry);
        }
        section.append(people);
      }
      section.append(external(motion.history_url, 'Matter history ↗'));
      section.append(external(motion.item_url, 'Meeting item ↗'));
      if (motion.block_url) section.append(external(motion.block_url, 'Consent agenda ↗'));
      section.append(external(motion.votes_url, 'Recorded votes ↗'));
      const proof = node('details', 'proof');
      proof.append(node('summary', '', 'Action capture details'));
      proof.append(node('p', '', `History locator: ${motion.history_locator} · captured ${motion.captured_at}`));
      proof.append(node('p', 'digest', `History SHA-256 ${motion.history_sha256}`));
      proof.append(node('p', 'digest', `Meeting item SHA-256 ${motion.item_sha256}`));
      proof.append(node('p', 'digest', `Votes SHA-256 ${motion.votes_sha256}`));
      if (motion.block_sha256) proof.append(node('p', 'digest', `Consent agenda SHA-256 ${motion.block_sha256}`));
      section.append(proof); card.append(section);
    }
    if (item.people_context?.length) {
      const people = node('div', 'people');
      people.append(node('p', 'meta', 'Officials named nearby. Their individual votes on this item are not established.'));
      for (const person of item.people_context) {
        const entry = node('div', 'person');
        if (person.portrait) {
          const picture = node('img', 'person__photo');
          picture.src = person.portrait.src;
          picture.alt = `Official portrait of ${person.name}`;
          picture.loading = 'lazy';
          entry.append(picture);
        }
        const detail = node('div', 'person__details');
        detail.append(node('strong', '', person.name));
        detail.append(node('p', 'meta', `${person.role_in_source} · Individual vote not established`));
        if (person.portrait) {
          detail.append(external(person.portrait.image_url, 'Open official image ↗'));
          detail.append(external(person.portrait.source_url, 'Portrait source ↗'));
        }
        entry.append(detail);
        people.append(entry);
      }
      card.append(people);
    }
    card.append(node('p', 'note', 'Not yet verified as a specific approval, individual vote, signed contract, or deployment.'));
    if (item.kind === 'agenda_portal_reference') card.append(node('p', 'meta', 'This county-linked portal includes other public bodies; the responsible body for this reference is not established.'));
    const sourceLabel = item.kind === 'meeting_document' ? 'Open captured meeting document ↗' :
      item.kind === 'agenda_portal_reference' ? 'Open official meeting page ↗' : 'Open captured source query ↗';
    card.append(external(item.source_url, sourceLabel));
    if (item.matter_url) card.append(external(item.matter_url, 'Open official matter ↗'));
    const detail = node('details', 'proof');
    detail.append(node('summary', '', 'Capture details'));
    detail.append(node('p', '', `Locator: ${item.locator}`));
    detail.append(node('p', '', `Captured: ${item.captured_at}`));
    detail.append(node('p', 'digest', `SHA-256 ${item.source_sha256}`));
    card.append(detail);
    results.append(card);
  }
}

try {
  const response = await fetch('./public/research-signals.json', { cache: 'no-store' });
  const report = await readBoundedResearchResponse(response);
  summary.textContent = `${report.records.length.toLocaleString()} preliminary references · ${report.coverage.jurisdictions_with_mentions} jurisdictions · ${(report.coverage.recorded_vote_entries ?? 0).toLocaleString()} recorded vote entries · complete search coverage is not established`;
  search.addEventListener('input', () => render(report));
  render(report);
} catch {
  summary.textContent = 'Research references are temporarily unavailable.';
}
