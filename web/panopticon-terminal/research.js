import { matchesResearchQuery, readBoundedResearchResponse, researchReferenceScope, validResearchJurisdiction } from './research-api.js?v=sterling-context-1';
import { votePortraits } from './portraits/portraits.js';
import { lookupResearchPortrait } from './portraits/lookup.js';

const summary = document.querySelector('#research-summary');
const scope = document.querySelector('#research-scope');
const search = document.querySelector('#research-search');
const count = document.querySelector('#research-count');
const results = document.querySelector('#research-results');
const selected = new URLSearchParams(location.search).get('jurisdiction');
const jurisdiction = validResearchJurisdiction(selected) ? selected : null;
const selectedRecord = new URLSearchParams(location.search).get('record');
const recordId = /^co-[a-z0-9-]{1,150}$/.test(selectedRecord ?? '') ? selectedRecord : null;
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
  const matches = report.records.filter((item) => (!jurisdiction || item.jurisdiction_ref === jurisdiction) &&
    (!recordId || item.id === recordId) && matchesResearchQuery(item, search.value));
  const latest = (item) => (item.actions ?? []).reduce((date, action) =>
    /^\d{4}-\d{2}-\d{2}$/.test(action.date) && action.date > date ? action.date : date, '');
  matches.sort((left, right) => latest(right).localeCompare(latest(left)) || left.jurisdiction.localeCompare(right.jurisdiction));
  count.textContent = `${matches.length.toLocaleString()} ${matches.length === 1 ? 'match' : 'matches'}${recordId ? ' for this reference' : jurisdiction ? ' linked to this jurisdiction' : ''}${matches.length > 100 ? ' · showing the first 100' : ''}`;
  results.replaceChildren();
  for (const item of matches.slice(0, 100)) {
    const card = node('article', 'card');
    card.id = item.id;
    card.append(node('p', 'label', item.kind === 'official_page_context' ? (item.context_state === 'withdrawn' ? 'CONTEXT WITHDRAWN · NO DECISION ESTABLISHED' : 'OFFICIAL AGENCY CONTEXT · NO DECISION ESTABLISHED') : 'PRELIMINARY SOURCE REFERENCE'));
    const sourceScope = researchReferenceScope(item);
    card.append(node('h2', '', sourceScope.heading));
    if (item.file) card.append(node('p', 'file', item.file));
    card.append(node('p', 'title', item.title || 'A relevant term was found in this source.'));
    card.append(node('p', 'signals', item.signals.join(' · ')));
    if (sourceScope.notice) card.append(node('p', 'meta', sourceScope.notice));
    if (item.source_status) card.append(node('p', 'meta', `The source lists its status as “${item.source_status}.” This is not an individual roll call.`));
    if (item.historical) card.append(node('p', 'meta', 'Historical capture: a later source check changed this record or no longer found the same reference.'));
    if (item.ocr) card.append(node('p', 'meta', 'Text was read with OCR and may contain errors.'));
    if (item.document_highlights?.length) {
      const details = node('details', 'proof');
      details.open = true;
      details.append(node('summary', '', 'Other matching passages in this document'));
      details.append(node('p', 'meta', 'Up to five additional source passages; these do not establish an individual vote.'));
      const list = node('ul', '');
      for (const highlight of item.document_highlights) {
        list.append(node('li', '', `${highlight.locator}: ${highlight.title}`));
      }
      details.append(list); card.append(details);
    }
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
            const portraitSource = portrait.source_kind === 'embedded_pdf'
              ? `${portrait.source_url}#page=${portrait.source_page}` : portrait.source_url;
            detail.append(external(portraitSource, 'Portrait source ↗'));
            if (portrait.identity_source_url) detail.append(external(portrait.identity_source_url, 'Name identification ↗'));
            if (portrait.identity_agenda_url) {
              detail.append(external(portrait.identity_agenda_url, 'Appointment agenda (third-party copy) ↗'));
              detail.append(external(portrait.identity_matter_url, 'City appointment record ↗'));
              detail.append(node('p', 'meta', portrait.caption));
              detail.append(node('p', 'meta digest', `Appointment agenda SHA-256 ${portrait.identity_agenda_sha256}`));
              detail.append(node('p', 'meta digest', `Fire board page SHA-256 ${portrait.portrait_source_sha256}`));
              detail.append(node('p', 'meta digest', `City authority roster SHA-256 ${portrait.identity_source_sha256}`));
              detail.append(node('p', 'meta digest', `City appointment record SHA-256 ${portrait.identity_matter_sha256}`));
            }
            if (portrait.source_kind === 'embedded_pdf') {
              const portraitProof = node('details', 'proof');
              portraitProof.append(node('summary', '', 'Portrait capture details'));
              portraitProof.append(node('p', 'meta', portrait.caption));
              portraitProof.append(node('p', 'meta', portrait.image_transform === 'lossless_png_from_jpeg2000'
                ? `Portrait on source page ${portrait.source_page}, converted losslessly from JPEG 2000 to PNG for display.`
                : `Original portrait embedded on source page ${portrait.source_page}.`));
              portraitProof.append(node('p', 'meta digest', `Source SHA-256 ${portrait.source_document_sha256}`));
              if (portrait.embedded_original_sha256) portraitProof.append(node('p', 'meta digest', `Embedded image SHA-256 ${portrait.embedded_original_sha256}`));
              detail.append(portraitProof);
            }
            detail.append(node('p', 'meta', 'Portrait date not established.'));
            if (portrait.source_conflict?.public_note) detail.append(node('p', 'meta', portrait.source_conflict.public_note));
          } else detail.append(node('p', 'meta', 'Portrait not yet established.'));
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
      people.append(node('p', 'meta', 'Officials named near the main passage. Other passages may concern different items or meetings; their individual votes are not established.'));
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
        } else detail.append(node('p', 'meta', 'Portrait not yet established.'));
        entry.append(detail);
        people.append(entry);
      }
      card.append(people);
    }
    const hasRecordedVotes = (item.actions ?? []).some((motion) => motion.votes.length > 0);
    card.append(node('p', 'note', item.kind === 'official_page_context'
      ? (item.context_state === 'withdrawn' ? 'The captured agency page changed and no longer matches our validation. No approval or individual vote was established.' : 'This agency page describes technology. It does not establish an approval decision, contract, or individual vote.') : hasRecordedVotes
      ? 'Recorded votes are shown with their scope above. A specific contract approval or deployment is not yet established.'
      : 'Not yet verified as a specific approval, individual vote, signed contract, or deployment.'));
    if (item.kind === 'agenda_portal_reference') card.append(node('p', 'meta', 'This county-linked portal includes other public bodies; the responsible body for this reference is not established.'));
    if (!sourceScope.notice && item.kind === 'meeting_document' && new URL(item.indexed_source_url ?? item.source_url).hostname === 'towncloud.io') {
      card.append(node('p', 'meta', 'The government website links this meeting document; the responsible public body is not established.'));
    }
    const sourceLabel = item.kind === 'meeting_document' ? `Open live ${item.document_format === 'docx' ? 'Word' : 'PDF'} meeting document ↗` :
      item.kind === 'agenda_portal_reference' ? 'Open official meeting page ↗' : item.kind === 'official_page_context' ? 'Open official agency page ↗' : 'Open live source query ↗';
    card.append(external(item.source_url, sourceLabel));
    card.append(node('p', 'meta', 'The live source may have changed since capture; the recorded digest is below.'));
    if (item.capture_adjustment) {
      if (item.capture_adjustment === 'timestamp_omitted') {
        card.append(node('p', 'meta', 'The indexed link included a timestamp; the captured document uses the same path without it. The versions may differ.'));
      }
      card.append(external(item.indexed_source_url, 'Original indexed link ↗'));
      card.append(external(item.source_index_url, 'Government agenda index ↗'));
    }
    if (item.springs_source_chain) {
      card.append(external(item.springs_source_chain.original_index_url, 'Original labeled city index ↗'));
      card.append(external(item.springs_source_chain.original_document_url, 'Original indexed document link ↗'));
      card.append(external(item.springs_source_chain.landing_url, 'City document landing page ↗'));
    }
    if (item.matter_url) card.append(external(item.matter_url, 'Open official matter ↗'));
    const permalink = node('a', 'source', 'Link to this reference ↗');
    const referenceUrl = new URL('./research.html', location.href);
    referenceUrl.searchParams.set('record', item.id);
    permalink.href = referenceUrl.href;
    card.append(permalink);
    const detail = node('details', 'proof');
    detail.append(node('summary', '', 'Capture details'));
    detail.append(node('p', '', `Locator: ${item.locator}`));
    detail.append(node('p', '', `Captured: ${item.captured_at}`));
    detail.append(node('p', 'digest', `SHA-256 ${item.source_sha256}`));
    if (item.springs_source_chain) {
      const chain = item.springs_source_chain;
      detail.append(node('p', '', `Original index anchor ${chain.original_anchor_index}; landing-page anchor ${chain.landing_anchor_index}.`));
      detail.append(node('p', 'digest', `Original index SHA-256 ${chain.original_index_sha256}`));
      detail.append(node('p', 'digest', `Landing page SHA-256 ${chain.landing_sha256}`));
      detail.append(node('p', 'digest', `Archived HTTP ${chain.redirect_status} receipt SHA-256 ${chain.redirect_receipt_sha256}; captured ${chain.redirect_captured_at}.`));
    }
    card.append(detail);
    results.append(card);
  }
}

try {
  const response = await fetch('./public/research-signals.json', { cache: 'no-store' });
  const report = await readBoundedResearchResponse(response);
  const ocrStatus = report.coverage.ocr_revalidation_pending === undefined ? 'OCR rechecks not recorded' :
    `${report.coverage.ocr_revalidation_pending.toLocaleString()} older scans awaiting OCR recheck`;
  const activeReferences = report.records.filter((item) => item.context_state !== 'withdrawn').length;
  const withdrawnContexts = report.records.length - activeReferences;
  summary.textContent = `${activeReferences.toLocaleString()} preliminary references${withdrawnContexts ? ` · ${withdrawnContexts} withdrawn context${withdrawnContexts === 1 ? '' : 's'}` : ''} · ${report.coverage.jurisdictions_with_mentions} jurisdictions with linked references · ${(report.coverage.recorded_vote_entries ?? 0).toLocaleString()} recorded vote entries · ${ocrStatus} · complete search coverage is not established`;
  if (jurisdiction) {
    const attempt = report.coverage.search_attempts?.find((item) => item.jurisdiction_ref === jurisdiction);
    scope.hidden = false;
    scope.textContent = attempt ?
      `Website discovery ${attempt.website_check === 'site_checked' ? 'reached the site' : 'was unavailable'} on ${attempt.checked_at.slice(0, 10)}. ` +
        `${attempt.documents_screened} documents screened · ${attempt.documents_failed} incomplete${attempt.ocr_revalidation_pending === undefined ? '' : ` (${attempt.ocr_revalidation_pending} awaiting OCR recheck)`} · ${attempt.documents_queued} queued${attempt.documents_redirected === undefined ? '' : ` · ${attempt.documents_redirected} source redirects`} · ` +
        `${attempt.portal_pages_checked} portal query pages checked · ` +
        `${attempt.historical_index_pages_checked === undefined ? 'historical index checks not recorded' : `${attempt.historical_index_pages_checked} historical index pages checked`} · ` +
        `${attempt.historical_index_errors === undefined ? 'historical index failures not recorded' : `${attempt.historical_index_errors} historical index checks failed`} · ` +
        `${attempt.agenda_meeting_pages_screened} agenda-platform meeting pages screened. ` +
        'These are partial source checks; the dates covered are not established.' :
      'No website discovery attempt is recorded for this jurisdiction. Its search coverage is not established.';
  }
  search.addEventListener('input', () => render(report));
  render(report);
} catch {
  summary.textContent = 'Research references are temporarily unavailable.';
}
