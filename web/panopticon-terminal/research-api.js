// Preliminary source mentions are a separate dataset from verified decisions.
const onlyKeys = (value, keys) => value && typeof value === 'object' && !Array.isArray(value) &&
  Object.keys(value).every((key) => keys.includes(key));
const nonnegative = (value) => Number.isSafeInteger(value) && value >= 0;
const digest = (value) => typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);
const safeText = (value, maximum) => typeof value === 'string' && value.length <= maximum &&
  !/[\x00-\x1f\x7f-\x9f\u202a-\u202e\u2066-\u2069]/u.test(value);
const locator = (value) => typeof value === 'string' && /^\$\[\d+\]$/.test(value);

export function matchesResearchQuery(item, value) {
  const query = value.trim().toLocaleLowerCase();
  const people = [
    ...(item.people_context ?? []).map((person) => person.name),
    ...(item.actions ?? []).flatMap((action) => action.votes.map((vote) => vote.name)),
  ];
  return `${item.jurisdiction} ${item.file} ${item.title} ${item.signals.join(' ')} ${people.join(' ')}`.toLocaleLowerCase().includes(query);
}

function validatePortrait(portrait, context = null) {
  if (!onlyKeys(portrait, ['src', 'sha256', 'source_url', 'image_url', 'caption', 'portrait_date',
      'source_kind', 'source_document_sha256', 'source_page', 'embedded_image_index']) ||
      !digest(portrait.sha256) ||
      !new RegExp(`^\\./research-portraits/${portrait.sha256}\\.(?:jpg|png|webp)$`).test(portrait.src) ||
      portrait.portrait_date !== 'not_established' || !safeText(portrait.caption, 250)) throw new Error('Invalid research portrait');
  let page;
  try { page = new URL(portrait.source_url); }
  catch { throw new Error('Invalid portrait source URL'); }
  if (page.protocol !== 'https:' ||
      !page.hostname.includes('.') || /^(?:localhost|\[|[0-9.]+$)/i.test(page.hostname) ||
      /\.(?:localhost|local|internal)$/i.test(page.hostname) ||
      page.username || page.password || page.port || page.hash) {
    throw new Error('Unsafe portrait source URL');
  }
  if (portrait.source_kind === 'embedded_pdf') {
    const identities = new Map([
      [513, ['Carrie Warren-Gully', 6, '84202caa2919b815df89984e12ad15f4de4289a35b74f048f54f0e68f0954174', 'Carrie Warren Gully']],
      [225, ['Nancy Sharpe', 7, '6555e2248f94d78cd16f6f4688f61c69da5029755f3da2ed9a982f6639588a8e', 'Nancy N. Sharpe']],
      [226, ['Jeff Baker', 8, '62aa905c15672fba1f48abd5a356e03dda7f2492077dab2a77e4543ae9063314', 'Jeff Baker']],
      [227, ['Nancy Jackson', 9, '50405246a18ce227b0671befb2a0e5148e83d3e54acdec20d3e3d1cf0c2122da', 'Nancy Jackson']],
      [228, ['Bill Holen', 10, '6023e0418ecc3a5f92ee2523482d6941de9156fd567cf46f3e9c6dd25370516e', 'Bill L. Holen']],
    ]);
    const expected = identities.get(context?.personId);
    if (!expected || context.client !== 'arapahoe' || context.jurisdiction !== 'us-co-county-08005' ||
        context.name !== expected[0] || portrait.embedded_image_index !== expected[1] || portrait.sha256 !== expected[2] ||
        portrait.caption !== `2022 county board; source labels this portrait ${expected[3]}.` ||
        portrait.image_url !== undefined || portrait.source_page !== 7 ||
        portrait.source_document_sha256 !== 'dd6476e64941674e310110b375d90dcf9f81ef904f0866183337b2b0fb867c43' ||
        portrait.source_url !== 'https://cms3.revize.com/revize/arapahoe/Finance/Complete%20Budget%20Document/2023%20Recommended%20Budget.pdf') {
      throw new Error('Embedded portrait does not match the recorded voter and source');
    }
    return;
  }
  if (portrait.source_kind !== undefined || portrait.source_document_sha256 !== undefined ||
      portrait.source_page !== undefined || portrait.embedded_image_index !== undefined) throw new Error('Unknown portrait source kind');
  let image;
  try { image = new URL(portrait.image_url); }
  catch { throw new Error('Invalid portrait image URL'); }
  if (image.protocol !== 'https:' || page.hostname !== image.hostname || image.username || image.password || image.port || image.hash) {
    throw new Error('Unsafe portrait image URL');
  }
}

export function validateResearch(report) {
  if (!onlyKeys(report, ['schema_version', 'state', 'generated_at', 'notice', 'coverage', 'records']) ||
      report.schema_version !== 'pnull-research-v1' || report.state !== 'preliminary' ||
      !Array.isArray(report.records) || report.records.length > 50000 ||
      !onlyKeys(report.coverage, ['lookback_period', 'jurisdictions_with_mentions', 'matter_references',
        'document_references', 'agenda_portal_references', 'agenda_portal_screening', 'search_attempts', 'recorded_actions', 'recorded_vote_entries', 'document_screening',
        'refresh_failures_preserving_prior_results', 'non_surveillance_mentions_withheld', 'omitted_failed_provenance']) ||
      report.coverage.lookback_period !== null ||
      !onlyKeys(report.coverage.document_screening, ['pending', 'error', 'matched', 'no_match']) ||
      !Object.values(report.coverage.document_screening).every(nonnegative) ||
      (report.coverage.agenda_portal_screening !== undefined &&
        (!onlyKeys(report.coverage.agenda_portal_screening, ['pending', 'error', 'matched', 'no_match']) ||
          !Object.values(report.coverage.agenda_portal_screening).every(nonnegative))) ||
      !nonnegative(report.coverage.omitted_failed_provenance) ||
      (report.coverage.non_surveillance_mentions_withheld !== undefined &&
        !nonnegative(report.coverage.non_surveillance_mentions_withheld)) ||
      (report.coverage.refresh_failures_preserving_prior_results !== undefined &&
        !nonnegative(report.coverage.refresh_failures_preserving_prior_results)) ||
      typeof report.notice !== 'string' || report.notice.length > 500 ||
      /[\x00-\x1f\x7f-\x9f\u202a-\u202e\u2066-\u2069]/u.test(report.notice) ||
      typeof report.generated_at !== 'string' || !Number.isFinite(Date.parse(report.generated_at))) {
    throw new Error('Invalid preliminary research report');
  }
  const seen = new Set();
  if (report.coverage.search_attempts !== undefined) {
    if (!Array.isArray(report.coverage.search_attempts) || report.coverage.search_attempts.length > 5000) throw new Error('Invalid research attempts');
    const places = new Set();
    for (const attempt of report.coverage.search_attempts) {
      if (!onlyKeys(attempt, ['jurisdiction_ref', 'checked_at', 'website_check', 'documents_screened', 'documents_failed',
        'documents_queued', 'portal_pages_checked', 'agenda_meeting_pages_screened', 'lookback_period']) ||
          !/^us-co-[a-z0-9-]{1,100}$/.test(attempt.jurisdiction_ref) || places.has(attempt.jurisdiction_ref) ||
          typeof attempt.checked_at !== 'string' || !Number.isFinite(Date.parse(attempt.checked_at)) ||
          !['site_checked', 'unavailable'].includes(attempt.website_check) || attempt.lookback_period !== null ||
          ![attempt.documents_screened, attempt.documents_failed, attempt.documents_queued, attempt.portal_pages_checked,
            attempt.agenda_meeting_pages_screened].every(nonnegative)) throw new Error('Invalid research attempt');
      places.add(attempt.jurisdiction_ref);
    }
  }
  for (const record of report.records) {
    if (!onlyKeys(record, ['id', 'kind', 'confidence', 'jurisdiction_ref', 'jurisdiction', 'file', 'title', 'signals',
      'source_status', 'source_url', 'matter_url', 'source_sha256', 'locator', 'captured_at', 'last_screened_at',
      'people_context', 'historical', 'ocr', 'actions', 'document_format']) ||
        !/^co-[a-z0-9-]{1,150}$/.test(record.id) || seen.has(record.id) ||
        !['matter_reference', 'meeting_document', 'agenda_portal_reference'].includes(record.kind) ||
        record.confidence !== 'unconfirmed_source_reference' ||
        !/^us-co-[a-z0-9-]{1,100}$/.test(record.jurisdiction_ref) ||
        typeof record.jurisdiction !== 'string' || !record.jurisdiction.trim() || record.jurisdiction.length > 300 ||
        typeof record.title !== 'string' || record.title.length > 400 ||
        typeof record.file !== 'string' || record.file.length > 120 ||
        typeof record.source_status !== 'string' || record.source_status.length > 100 ||
        !Array.isArray(record.signals) || !record.signals.length || record.signals.length > 24 ||
        record.signals.some((item) => typeof item !== 'string' || !item.trim() || item.length > 100) ||
        !/^[0-9a-f]{64}$/.test(record.source_sha256) ||
        !/^(?:\$\[\d+\]\.MatterTitle|extracted line [1-9]\d*|HTML text node [1-9]\d*)$/.test(record.locator) ||
        typeof record.captured_at !== 'string' || !Number.isFinite(Date.parse(record.captured_at)) ||
        typeof record.last_screened_at !== 'string' || !Number.isFinite(Date.parse(record.last_screened_at))) {
      throw new Error('Invalid preliminary research record');
    }
    seen.add(record.id);
    for (const value of [record.jurisdiction, record.title, record.file, record.source_status, ...record.signals]) {
      if (/[\x00-\x1f\x7f-\x9f\u202a-\u202e\u2066-\u2069]/u.test(value)) throw new Error('Unsafe research text');
    }
    const parsedUrls = [];
    for (const value of [record.source_url, ...(record.matter_url ? [record.matter_url] : [])]) {
      if (typeof value !== 'string') throw new Error('Invalid research source URL');
      let url;
      try { url = new URL(value); } catch { throw new Error('Invalid research source URL'); }
      if (url.protocol !== 'https:' || !url.hostname.includes('.') || url.username || url.password || url.hash ||
          url.port || /^(?:localhost|\[|[0-9.]+$)/i.test(url.hostname) || /\.(?:localhost|local|internal)$/i.test(url.hostname) || value.length > 4000) {
        throw new Error('Unsafe research source URL');
      }
      if (record.kind === 'matter_reference' && url.hostname !== 'webapi.legistar.com') {
        throw new Error('Matter reference is not from the official portal API');
      }
      parsedUrls.push(url);
    }
    if (record.kind === 'matter_reference' && !record.matter_url) throw new Error('Matter reference lacks an official record URL');
    if (record.kind === 'matter_reference') {
      const source = /^\/v1\/([a-z0-9-]+)\/matters$/.exec(parsedUrls[0].pathname);
      const matter = /^\/v1\/([a-z0-9-]+)\/Matters\/([1-9]\d*)$/.exec(parsedUrls[1].pathname);
      if (!source || !matter || source[1] !== matter[1] || parsedUrls[1].search ||
          record.id !== `co-legistar-${matter[1]}-${matter[2]}`) throw new Error('Matter identity is inconsistent');
    } else if (record.kind === 'meeting_document' && (!/^co-document-[a-f0-9]{24}$/.test(record.id) || parsedUrls[0].search ||
        !(/\/AgendaCenter\/ViewFile\/(?:Agenda|Minutes)\/_[0-9]{8}-[0-9]{1,12}$/i.test(parsedUrls[0].pathname) ||
          /\.(?:pdf|docx)$/i.test(parsedUrls[0].pathname)))) throw new Error('Meeting document URL is inconsistent');
    else if (record.kind === 'agenda_portal_reference') {
      const meeting = /^\/iip\/elpaso\/meeting\/details\/([1-9]\d*)$/.exec(parsedUrls[0].pathname);
      if (parsedUrls[0].hostname !== 'www.agendasuite.org' || parsedUrls[0].search || !meeting ||
          record.jurisdiction_ref !== 'us-co-county-08041' || record.id !== `co-agendasuite-elpaso-${meeting[1]}-${record.source_sha256.slice(0, 12)}` ||
          !/^HTML text node [1-9]\d*$/.test(record.locator)) throw new Error('Agenda portal source is inconsistent');
    }
    if (record.kind !== 'matter_reference' && record.matter_url !== undefined) throw new Error('Unexpected matter URL');
    if (record.kind === 'meeting_document' && record.ocr !== undefined && typeof record.ocr !== 'boolean') throw new Error('Invalid OCR label');
    if (record.document_format !== undefined && (record.kind !== 'meeting_document' || !['pdf', 'docx'].includes(record.document_format))) throw new Error('Invalid document format');
    if (record.kind === 'meeting_document' && ((/\.docx$/i.test(parsedUrls[0].pathname) && record.document_format !== 'docx') ||
        (/\.pdf$/i.test(parsedUrls[0].pathname) && record.document_format === 'docx'))) throw new Error('Document format conflicts with its URL');
    if (record.historical !== undefined && typeof record.historical !== 'boolean') throw new Error('Invalid historical label');
    if (record.actions !== undefined) {
      if (record.kind !== 'matter_reference' || !Array.isArray(record.actions) || record.actions.length > 10) throw new Error('Invalid matter actions');
      const matter = /^\/v1\/([a-z0-9-]+)\/Matters\/([1-9]\d*)$/.exec(parsedUrls[1].pathname);
      const prefix = `https://webapi.legistar.com/v1/${matter[1]}`;
      for (const motion of record.actions) {
        if (!onlyKeys(motion, ['date', 'body', 'action', 'outcome', 'vote_scope', 'history_url', 'history_sha256',
          'history_locator', 'item_url', 'item_sha256', 'votes_url', 'votes_sha256', 'captured_at', 'votes', 'block_url', 'block_sha256', 'historical']) ||
            !/^(?:\d{4}-\d{2}-\d{2}|not_established)$/.test(motion.date) ||
            !safeText(motion.body, 200) || !safeText(motion.action, 200) || !safeText(motion.outcome, 100) ||
            !['item', 'consent_block', 'consent_designated_item', 'not_established'].includes(motion.vote_scope) ||
            motion.history_url !== `${prefix}/Matters/${matter[2]}/Histories` || !locator(motion.history_locator) ||
            !digest(motion.history_sha256) || !digest(motion.item_sha256) || !digest(motion.votes_sha256) ||
            (motion.historical !== undefined && typeof motion.historical !== 'boolean') ||
            typeof motion.captured_at !== 'string' || !Number.isFinite(Date.parse(motion.captured_at)) ||
            !Array.isArray(motion.votes) || motion.votes.length > 100 ||
            (motion.vote_scope === 'not_established' && motion.votes.length)) throw new Error('Invalid matter action');
        const escaped = matter[1].replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
        const item = new RegExp(`^https://webapi\\.legistar\\.com/v1/${escaped}/Events/([1-9]\\d*)/EventItems/([1-9]\\d*)$`).exec(motion.item_url);
        if (!item || motion.votes_url !== `${prefix}/EventItems/${item[2]}/Votes`) throw new Error('Invalid action source identity');
        if (motion.vote_scope === 'consent_block') {
          if (!new RegExp(`^${prefix.replaceAll('.', '\\.')}\\/Events/${item[1]}\\/EventItems/[1-9]\\d*$`).test(motion.block_url) ||
              !digest(motion.block_sha256)) throw new Error('Invalid consent-block evidence');
        } else if (motion.block_url !== undefined || motion.block_sha256 !== undefined) throw new Error('Unexpected consent-block evidence');
        const people = new Set();
        for (const vote of motion.votes) {
          if (!onlyKeys(vote, ['name', 'value', 'locator', 'person_id', 'vote_id', 'portrait']) ||
              !safeText(vote.name, 100) || !vote.name.trim() ||
              !['Aye', 'Yes', 'No', 'Nay', 'Abstain', 'Absent', 'Recused'].includes(vote.value) ||
              !locator(vote.locator) || !Number.isSafeInteger(vote.person_id) || vote.person_id <= 0 ||
              !Number.isSafeInteger(vote.vote_id) || vote.vote_id <= 0 || people.has(vote.person_id)) throw new Error('Invalid recorded vote');
          people.add(vote.person_id);
          if (vote.portrait) validatePortrait(vote.portrait, { client: matter[1], jurisdiction: record.jurisdiction_ref,
            personId: vote.person_id, name: vote.name });
        }
      }
    }
    if (record.people_context !== undefined) {
      if (record.kind !== 'meeting_document' || !Array.isArray(record.people_context) || record.people_context.length > 15) throw new Error('Invalid named context');
      for (const person of record.people_context) {
        if (!onlyKeys(person, ['name', 'role_in_source', 'name_locator', 'context_locator', 'individual_vote', 'portrait']) ||
            typeof person.name !== 'string' || !person.name.trim() || person.name.length > 100 ||
            typeof person.role_in_source !== 'string' || person.role_in_source.length > 50 ||
            !/^extracted line [1-9]\d*$/.test(person.name_locator) ||
            !/^extracted line [1-9]\d*$/.test(person.context_locator) || person.individual_vote !== 'not_established' ||
            /[\x00-\x1f\x7f-\x9f\u202a-\u202e\u2066-\u2069]/u.test(person.name + person.role_in_source)) throw new Error('Invalid named context');
        if (person.portrait) {
          validatePortrait(person.portrait);
        }
      }
    }
  }
  const count = (kind) => report.records.filter((item) => item.kind === kind).length;
  if (report.coverage.matter_references !== count('matter_reference') ||
      report.coverage.document_references !== count('meeting_document') ||
      (report.coverage.agenda_portal_references ?? 0) !== count('agenda_portal_reference') ||
      report.coverage.jurisdictions_with_mentions !== new Set(report.records.map((item) => item.jurisdiction_ref)).size) {
    throw new Error('Research counts do not match records');
  }
  const motions = report.records.flatMap((item) => item.actions ?? []);
  if ((report.coverage.recorded_actions !== undefined && report.coverage.recorded_actions !== motions.length) ||
      (report.coverage.recorded_vote_entries !== undefined && report.coverage.recorded_vote_entries !== motions.reduce((sum, motion) => sum + motion.votes.length, 0))) {
    throw new Error('Research action counts do not match records');
  }
  return report;
}

export async function readBoundedResearchResponse(response, maximum = 8 * 1024 * 1024) {
  if (!response.ok || !response.body || !Number.isSafeInteger(maximum) || maximum <= 0) {
    throw new Error('Research response unavailable');
  }
  const declared = Number(response.headers.get('content-length') ?? 0);
  if (!Number.isFinite(declared) || declared < 0 || declared > maximum) throw new Error('Research response exceeds its limit');
  const reader = response.body.getReader();
  const chunks = [];
  let size = 0;
  try {
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > maximum) throw new Error('Research response exceeds its limit');
      chunks.push(value);
    }
  } finally { await reader.cancel().catch(() => {}); }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
  return validateResearch(JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes)));
}
