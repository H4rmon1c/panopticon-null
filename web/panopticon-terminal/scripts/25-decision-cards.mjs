import { portraitForRecordedVote } from '../portraits/portraits.js';

// Friendly descriptions are attached to specific source-verified decisions;
// unknown future decisions use their published name and summary instead.
// Only use an editorial rewrite while the reviewed source text still matches.
// A correction to the published snapshot must immediately take precedence.
const copy = Object.freeze({
  'co-castle-rock-res-2025-071-2025-06-03': {
    digest: 'sha256:4cddd5b3a65b537df43d520c882c35ffdcdfd558c5e0b05c450e2f886a9b5511',
    expected: 'Castle Rock, CO approved RES 2025-071: Flock Safety Drone-As-First-Responder Program on 2025-06-03. The direct roll call records 6 Yes, 1 Abstain. Council approval of a sole-source agreement is documented. Town execution and payments are not established by these records.',
    title: 'Castle Rock approved a drone-response agreement',
    summary: 'Council approved a sole-source agreement with Flock. The records do not establish whether the town signed it or paid.'
  },
  'co-colorado-springs-25-581-2025-11-25': {
    digest: 'sha256:64e23d56e655f81c4b525af97f1d0712dce63bc7f8b911bd793c9695e8deca59',
    expected: 'The City Council finally passed Ordinance 25-93, amending city code to assess a Police Department Technology Surcharge.',
    title: 'Colorado Springs approved a police technology surcharge',
    summary: 'City Council passed a police technology surcharge. This was not itself a vote on a Flock or Axon contract.'
  },
  'co-commerce-city-ord-2755-2026-04-06': {
    digest: 'sha256:027f64e7735eae264c7c1dd69b384921c253641c2dab8224f3e2e0ac3fe1cb37',
    expected: 'Commerce City, Colorado approved Ord 2755: Flock Group, Inc. on 2026-04-06. The direct roll call records 7 Aye, 2 Nay. Council approval of the attached agreement format is documented. Signed city execution and payments are not established by these records.',
    title: 'Commerce City approved a Flock agreement format',
    summary: 'Council approved the format of a Flock agreement. A signed agreement or payments are not established.'
  },
  'co-commerce-city-ord-2756-2026-04-06': {
    digest: 'sha256:056ed3d074e8b132dc0721c933b83f633e76f57a32fba3a404c7690cbd3e5a17',
    expected: 'Commerce City, Colorado approved Ord 2756: Flock Expansion Project on 2026-04-06. The direct roll call records 7 Aye, 2 Nay. The $608,900 appropriation does not itself establish a signed agreement, payment, or equipment deployment.',
    title: 'Commerce City approved funding for a Flock expansion',
    summary: 'Council approved a $608,900 appropriation for a Flock expansion. Payment and deployment are not established.'
  },
  'co-denver-25-0413-2025-05-05': {
    digest: 'sha256:e8b498f58d7210ed3fd13ce8c6906d4855b894035fa4db03a8f996a0b595852c',
    expected: 'City and County of Denver did not approve 25-0413: Flock Group, Inc. on 2025-05-05. The direct roll call records 12 Nay, 1 Absent. This proposed amendment failed. The recorded vote does not approve that additional amount, a new execution, or payment.',
    title: 'Denver rejected a proposed Flock contract amendment',
    summary: 'A proposed contract amendment failed. This vote did not approve additional spending.'
  },
});

export function decisionHref(record, mode = 'public') {
  const slug = String(record.slug ?? '');
  if (!/^[a-z0-9-]+$/.test(slug)) return null;
  if (mode === 'demo' || mode === 'preview') return `./decision.html?decision=${encodeURIComponent(slug)}&${mode}=1`;
  return `./decisions/${slug}.html`;
}

export function displayVote(value) {
  const raw = String(value ?? '').trim();
  return /^(aye|yes)$/i.test(raw) ? 'Yes' : /^(nay|no)$/i.test(raw) ? 'No' : /^absent$/i.test(raw) ? 'Absent' : /^abstain(?:ed)?$/i.test(raw) ? 'Abstained' : (raw || 'Unknown');
}

export function voteSummary(record) {
  if (record.consent_block) return 'Consent agenda vote — see the full record';
  const votes = Array.isArray(record.votes) ? record.votes : [];
  if (!votes.length) return 'Individual votes not established';
  const counts = new Map();
  for (const item of votes) {
    const key = displayVote(item.vote ?? item.position);
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  const order = ['Yes', 'No', 'Abstained', 'Absent', 'Unknown'];
  return [...counts].sort(([a], [b]) => {
    const ai = order.indexOf(a); const bi = order.indexOf(b);
    return (ai < 0 ? order.length : ai) - (bi < 0 ? order.length : bi);
  }).map(([label, count]) => `${count} ${label}`).join(' · ');
}

export function outcomeLabel(record) {
  if (record.consent_block) return 'Consent agenda';
  const raw = (record.attributes ?? []).find((item) => /^(?:recorded )?outcome$/i.test(item.label ?? ''))?.value ?? '';
  if (/^(pass|passed|approved)$/i.test(raw)) return 'Passed';
  if (/^(fail|failed|rejected)$/i.test(raw)) return 'Failed';
  return 'Outcome not established';
}

function element(tag, className, text) {
  const item = document.createElement(tag);
  if (className) item.className = className;
  if (text !== undefined) item.textContent = String(text);
  return item;
}

function showDate(value) {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(String(value ?? ''))) return 'Date not established';
  const date = new Date(`${value}T12:00:00Z`);
  return Number.isNaN(date.getTime()) ? 'Date not established' : new Intl.DateTimeFormat('en-US', {month:'short',day:'numeric',year:'numeric',timeZone:'UTC'}).format(date);
}

export function decisionCardModels(snapshot, mode = 'public') {
  const entities = Array.isArray(snapshot?.entities) ? snapshot.entities : [];
  const byId = new Map(entities.map((entity) => [entity.id, entity]));
  const releases = new Map((snapshot?.case_releases ?? []).map((item) => [item.case_id, item.release]));
  return entities.filter((entity) => String(entity.type).toUpperCase() === 'DECISION').map((record) => {
    const place = (record.jurisdiction_ids ?? []).map((id) => byId.get(id)?.name).find(Boolean) ?? 'Colorado';
    const votes = record.consent_block ? (record.consent_block.roll_call ?? []) : (record.votes ?? []);
    const people = votes.map((vote) => ({vote, portrait: mode === 'demo' ? null : portraitForRecordedVote(vote, byId.get(vote.voter_entity_id))}));
    const sourceText = record.summary ?? record.description ?? 'Open the record to see what the sources establish.';
    const outcome = outcomeLabel(record);
    const expectedOutcome = record.slug === 'co-denver-25-0413-2025-05-05' ? 'Failed' : 'Passed';
    const release = releases.get(record.slug);
    const editorial = copy[record.slug]?.expected === sourceText && outcome === expectedOutcome &&
      release?.bound_digest === copy[record.slug]?.digest && !(release?.corrections?.length) ? copy[record.slug] : null;
    const rawDate = record.date ?? record.timeline?.[0]?.date;
    return { record, place, title: editorial?.title ?? record.name ?? 'Public decision', summary: editorial?.summary ?? sourceText,
      rawDate, date: showDate(rawDate), outcome, tally: voteSummary(record), people, href: decisionHref(record, mode) };
  }).filter((card) => card.href).sort((a, b) => String(b.rawDate ?? '').localeCompare(String(a.rawDate ?? '')));
}

export function matchesDecisionCard(card, query) {
  const needle = String(query ?? '').trim().toLocaleLowerCase();
  if (!needle) return true;
  const search = [card.place, card.title, card.record.name, card.summary, ...card.people.map(({vote}) => vote.name ?? '')].join(' ').toLocaleLowerCase();
  return search.includes(needle.replace(/(?:,\s*|\s+)(?:co|colorado)\s*$/i, ''));
}

export function previewPeople(card, query) {
  const needle = String(query ?? '').trim().toLocaleLowerCase();
  if (!needle) return card.people.slice(0, 4);
  const matched = card.people.filter(({vote}) => String(vote.name ?? '').toLocaleLowerCase().includes(needle));
  if (!matched.length) return card.people.slice(0, 4);
  return [...matched, ...card.people.filter((person) => !matched.includes(person))].slice(0, 4);
}

export function linkedDecisions(snapshot, mode, entityId) {
  if (!entityId) return [];
  return decisionCardModels(snapshot, mode).flatMap((card) => {
    if (card.record.id === entityId) return [{ card, href: card.href }];
    return card.people.filter(({vote}) => vote.voter_entity_id === entityId).map(({vote}) => ({
      card, href: vote.evidence_id ? `${card.href}#evidence-${encodeURIComponent(vote.evidence_id)}` : `${card.href}#recorded-votes`
    }));
  });
}

function renderPerson(person, href) {
  const name = person.vote.name ?? 'Unnamed voter';
  const vote = displayVote(person.vote.vote ?? person.vote.position);
  const link = element('a', 'record-person');
  link.href = person.vote.evidence_id ? `${href}#evidence-${encodeURIComponent(person.vote.evidence_id)}` : `${href}#recorded-votes`;
  link.setAttribute('aria-label', `${name}: ${vote}. Open the recorded position.`);
  const face = element('span', 'record-person__face');
  const initials = name.split(/\s+/).filter(Boolean).slice(0,2).map((part) => part[0]).join('').toUpperCase();
  face.append(element('span', 'record-person__initials', initials || '?'));
  if (person.portrait) {
    const img = document.createElement('img');
    img.src = person.portrait.src;
    img.alt = `Portrait of ${name}`;
    img.loading = 'lazy'; img.decoding = 'async';
    img.addEventListener('error', () => img.remove(), {once:true});
    face.append(img);
  }
  link.append(face, element('strong', 'record-person__name', name), element('span', 'record-person__vote', String(vote)));
  return link;
}

export function renderDecisionCards(snapshot, mode, query, list, counter) {
  const cards = decisionCardModels(snapshot, mode).filter((card) => matchesDecisionCard(card, query));
  list.replaceChildren();
  if (counter) counter.textContent = `${cards.length} released decision${cards.length === 1 ? '' : 's'}${String(query ?? '').trim() ? ' found' : ''}`;
  if (!cards.length) {
    const empty = element('div', 'record-empty');
    empty.append(element('strong', '', 'No released decisions found.'), element('p', '', 'That does not mean nothing happened. Try another name or search the Colorado directory for preliminary research.'));
    list.append(empty);
    return;
  }
  for (const card of cards) {
    const article = element('article', 'decision-card');
    const top = element('div', 'decision-card__top');
    top.append(element('p', 'decision-card__place', `${card.place}  ·  ${card.date}`), element('span', `decision-card__outcome ${card.outcome === 'Passed' ? 'is-passed' : card.outcome === 'Failed' ? 'is-failed' : ''}`, card.outcome));
    const title = element('h3', 'decision-card__title');
    const titleLink = element('a', '', card.title); titleLink.href = card.href; title.append(titleLink);
    article.append(top, title, element('p', 'decision-card__summary', card.summary), element('p', 'decision-card__official', `Official record: ${card.record.name}`));
    const votes = element('div', 'decision-card__votes');
    votes.append(element('span', 'decision-card__vote-label', card.record.consent_block ? 'Consent motion' : 'Recorded vote'), element('strong', 'decision-card__tally', card.tally));
    article.append(votes);
    if (card.people.length) {
      const people = element('div', 'decision-card__people');
      for (const person of previewPeople(card, query)) people.append(renderPerson(person, card.href));
      article.append(people);
      if (card.people.some(({portrait}) => portrait)) article.append(element('p', 'decision-card__photo-note', 'Photo sources are in the full record; photos may be from a different date.'));
    }
    const actions = element('div', 'decision-card__actions');
    const proof = element('a', 'decision-card__primary', 'Read decision & proof →'); proof.href = card.href;
    actions.append(proof);
    if (card.people.length) { const all = element('a', 'decision-card__secondary', `See all ${card.people.length} recorded positions`); all.href = `${card.href}#recorded-votes`; actions.append(all); }
    article.append(actions);
    list.append(article);
  }
}
